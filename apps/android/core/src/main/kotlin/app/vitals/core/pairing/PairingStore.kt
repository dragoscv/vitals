package app.vitals.core.pairing

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Base64
import android.util.Log
import app.vitals.core.WireJson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.SerializationException
import kotlinx.serialization.builtins.ListSerializer
import java.io.File
import java.security.KeyStore
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/**
 * The paired PCs, encrypted at rest with an Android Keystore AES-GCM key.
 *
 * A token is a bearer credential for someone's PC; a rooted phone or an
 * `adb backup` must not yield it in plain text. The key never leaves the
 * Keystore. The file is excluded from cloud backup (see the app's
 * `data_extraction_rules.xml`), because a token restored to another phone
 * would be a copy nobody paired.
 */
class PairingStore(context: Context) {
    private val file = File(context.noBackupFilesDir, "pairings.bin")
    private val mutex = Mutex()
    private val state = MutableStateFlow<List<Pairing>>(emptyList())
    private var loaded = false

    val pairings: StateFlow<List<Pairing>> = state.asStateFlow()

    suspend fun load(): List<Pairing> = mutex.withLock {
        if (!loaded) {
            // Null = a transient failure: the file is kept and the next load
            // tries again, rather than treating the pairings as gone.
            withContext(Dispatchers.IO) { read() }?.let {
                state.value = it
                loaded = true
            }
        }
        state.value
    }

    suspend fun upsert(pairing: Pairing) = update { list ->
        list.filterNot { it.id == pairing.id } + pairing
    }

    suspend fun remove(id: String) = update { list -> list.filterNot { it.id == id } }

    suspend fun update(transform: (List<Pairing>) -> List<Pairing>) {
        load()
        mutex.withLock {
            if (!loaded) {
                // Writing now would replace pairings we could not read with
                // an empty list plus this change — the loss AND-13 removed.
                Log.w(TAG, "pairings unreadable for now; change not saved")
                return
            }
            val next = transform(state.value)
            withContext(Dispatchers.IO) { write(next) }
            state.value = next
        }
    }

    private fun read(): List<Pairing>? {
        if (!file.exists()) return emptyList()
        return try {
            val bytes = file.readBytes()
            if (bytes.size <= IV_BYTES) return unrecoverable(null)
            val iv = bytes.copyOfRange(0, IV_BYTES)
            val cipher = Cipher.getInstance(TRANSFORM)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
            val plain = cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES)
            WireJson.Lenient.decodeFromString(ListSerializer(Pairing.serializer()), plain.decodeToString())
        } catch (e: KeyPermanentlyInvalidatedException) {
            // A key invalidated by a lock-screen reset, a file sealed by
            // another key or tampered with, or contents that are not
            // pairings: unrecoverable, so start empty rather than fail on
            // every launch. The user pairs again.
            unrecoverable(e)
        } catch (e: AEADBadTagException) {
            unrecoverable(e)
        } catch (e: SerializationException) {
            unrecoverable(e)
        } catch (e: IllegalArgumentException) {
            // kotlinx.serialization's other "not a valid instance" signal.
            unrecoverable(e)
        } catch (e: java.io.IOException) {
            // Storage hiccup, Keystore daemon busy after boot: the tokens are
            // still in the file, and deleting it would unpair every PC.
            Log.w(TAG, "pairings not readable right now", e)
            null
        } catch (e: java.security.GeneralSecurityException) {
            Log.w(TAG, "pairings not readable right now", e)
            null
        } catch (e: java.security.ProviderException) {
            Log.w(TAG, "pairings not readable right now", e)
            null
        }
    }

    private fun unrecoverable(e: Exception?): List<Pairing> {
        Log.w(TAG, "pairings unrecoverable; starting empty", e)
        file.delete()
        return emptyList()
    }

    private fun write(list: List<Pairing>) {
        val plain = WireJson.Lenient.encodeToString(ListSerializer(Pairing.serializer()), list)
        val cipher = Cipher.getInstance(TRANSFORM)
        cipher.init(Cipher.ENCRYPT_MODE, key())
        val sealed = cipher.iv + cipher.doFinal(plain.encodeToByteArray())
        val tmp = File(file.parentFile, "${file.name}.tmp")
        tmp.writeBytes(sealed)
        if (!tmp.renameTo(file)) {
            file.delete()
            tmp.renameTo(file)
        }
    }

    private fun key(): SecretKey {
        val ks = KeyStore.getInstance("AndroidKeyStore").apply { load(null) }
        (ks.getEntry(ALIAS, null) as? KeyStore.SecretKeyEntry)?.let { return it.secretKey }
        val gen = KeyGenerator.getInstance(KeyProperties.KEY_ALGORITHM_AES, "AndroidKeyStore")
        gen.init(
            KeyGenParameterSpec.Builder(ALIAS, KeyProperties.PURPOSE_ENCRYPT or KeyProperties.PURPOSE_DECRYPT)
                .setBlockModes(KeyProperties.BLOCK_MODE_GCM)
                .setEncryptionPaddings(KeyProperties.ENCRYPTION_PADDING_NONE)
                .setKeySize(256)
                .build(),
        )
        return gen.generateKey()
    }

    companion object {
        private const val TAG = "PairingStore"
        private const val ALIAS = "vitals.pairings.v1"
        private const val TRANSFORM = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12

        /** A short random id, stable for the life of a pairing. */
        fun newId(): String {
            val bytes = ByteArray(9).also { java.security.SecureRandom().nextBytes(it) }
            return Base64.encodeToString(bytes, Base64.URL_SAFE or Base64.NO_WRAP or Base64.NO_PADDING)
        }
    }
}
