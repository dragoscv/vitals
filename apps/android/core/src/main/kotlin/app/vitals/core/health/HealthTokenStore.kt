package app.vitals.core.health

import android.content.Context
import android.security.keystore.KeyGenParameterSpec
import android.security.keystore.KeyPermanentlyInvalidatedException
import android.security.keystore.KeyProperties
import android.util.Log
import app.vitals.core.WireJson
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.flow.MutableStateFlow
import kotlinx.coroutines.flow.StateFlow
import kotlinx.coroutines.flow.asStateFlow
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withContext
import kotlinx.serialization.Serializable
import kotlinx.serialization.SerializationException
import java.io.File
import java.security.KeyStore
import javax.crypto.AEADBadTagException
import javax.crypto.Cipher
import javax.crypto.KeyGenerator
import javax.crypto.SecretKey
import javax.crypto.spec.GCMParameterSpec

/** The vmui this phone reads health from, and the device token it was given. */
@Serializable
data class HealthCredentials(val base: String, val token: String) {
    // A data class prints its fields; the token must never reach a log line.
    override fun toString(): String = "HealthCredentials(base=$base, token=${HealthClient.tokenPrefix(token)}…)"
}

/**
 * The vmui health token, encrypted at rest with its own Android Keystore
 * AES-GCM key — the same pattern as [app.vitals.core.pairing.PairingStore],
 * with a separate alias so forgetting one never touches the other.
 *
 * The file sits in `noBackupFilesDir` and `data_extraction_rules.xml`
 * excludes every domain: a token restored to another phone would be a
 * device nobody paired. Health readings themselves are never stored.
 */
class HealthTokenStore(context: Context) {
    private val file = File(context.noBackupFilesDir, "health.bin")
    private val mutex = Mutex()
    private val state = MutableStateFlow<HealthCredentials?>(null)
    private var loaded = false

    val credentials: StateFlow<HealthCredentials?> = state.asStateFlow()

    suspend fun load(): HealthCredentials? = mutex.withLock {
        if (!loaded) {
            withContext(Dispatchers.IO) { read() }?.let {
                state.value = it.value
                loaded = true
            }
        }
        state.value
    }

    suspend fun save(credentials: HealthCredentials) = replace(credentials)

    suspend fun clear() = replace(null)

    private suspend fun replace(next: HealthCredentials?) {
        load()
        mutex.withLock {
            withContext(Dispatchers.IO) {
                if (next == null) file.delete() else write(next)
            }
            state.value = next
            loaded = true
        }
    }

    /** A box so "no file" (a value of null) differs from "cannot read now" (null box). */
    private class Read(val value: HealthCredentials?)

    private fun read(): Read? {
        if (!file.exists()) return Read(null)
        return try {
            val bytes = file.readBytes()
            if (bytes.size <= IV_BYTES) return unrecoverable(null)
            val iv = bytes.copyOfRange(0, IV_BYTES)
            val cipher = Cipher.getInstance(TRANSFORM)
            cipher.init(Cipher.DECRYPT_MODE, key(), GCMParameterSpec(128, iv))
            val plain = cipher.doFinal(bytes, IV_BYTES, bytes.size - IV_BYTES)
            Read(WireJson.Lenient.decodeFromString(HealthCredentials.serializer(), plain.decodeToString()))
        } catch (e: KeyPermanentlyInvalidatedException) {
            unrecoverable(e)
        } catch (e: AEADBadTagException) {
            unrecoverable(e)
        } catch (e: SerializationException) {
            unrecoverable(e)
        } catch (e: IllegalArgumentException) {
            unrecoverable(e)
        } catch (e: java.io.IOException) {
            // Transient (storage, Keystore busy after boot): keep the file.
            Log.w(TAG, "health token not readable right now", e)
            null
        } catch (e: java.security.GeneralSecurityException) {
            Log.w(TAG, "health token not readable right now", e)
            null
        } catch (e: java.security.ProviderException) {
            Log.w(TAG, "health token not readable right now", e)
            null
        }
    }

    private fun unrecoverable(e: Exception?): Read {
        Log.w(TAG, "health token unrecoverable; pair again", e)
        file.delete()
        return Read(null)
    }

    private fun write(credentials: HealthCredentials) {
        val plain = WireJson.Lenient.encodeToString(HealthCredentials.serializer(), credentials)
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
        private const val TAG = "HealthTokenStore"
        private const val ALIAS = "vitals.health.v1"
        private const val TRANSFORM = "AES/GCM/NoPadding"
        private const val IV_BYTES = 12
    }
}
