package app.vitals.phone.ui.settings

import android.os.Build
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.semantics.LiveRegionMode
import androidx.compose.ui.semantics.liveRegion
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.health.HealthClient
import app.vitals.core.health.HealthCredentials
import app.vitals.core.health.HealthFailure
import app.vitals.core.health.HealthResult
import app.vitals.core.health.HealthUrl
import app.vitals.core.health.PairState
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.components.SectionCard
import app.vitals.ui.Palette
import kotlinx.coroutines.Job
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

/** Where a connection attempt stands; drives the card's status line. */
private sealed interface Attempt {
    data object Idle : Attempt
    data object Working : Attempt
    data class Code(val code: String?) : Attempt
    data class Failed(val message: Int) : Attempt
}

private const val PAIR_POLL_MS = 3_000L
private const val PAIR_MAX_MS = 10 * 60_000L
private const val SLOW_DOWN_MS = 10_000L

/**
 * Connect this phone to the owner's vmui for the Health page
 * (docs/api/vmui-health.md): a four-digit code the owner approves in vmui,
 * or email and password once. The password lives in `remember` only — not
 * `rememberSaveable`, so it never lands in the saved-instance bundle — and
 * is cleared as soon as it has been sent.
 */
@Composable
internal fun HealthSettingsCard() {
    val graph = LocalContext.current.graph
    val scope = rememberCoroutineScope()
    val credentials by graph.health.credentials.collectAsStateWithLifecycle()
    var base by remember(credentials?.base) { mutableStateOf(credentials?.base ?: "") }
    var email by remember { mutableStateOf("") }
    var password by remember { mutableStateOf("") }
    var attempt by remember { mutableStateOf<Attempt>(Attempt.Idle) }
    var job by remember { mutableStateOf<Job?>(null) }
    var disconnecting by remember { mutableStateOf(false) }
    val deviceName = Build.MODEL?.takeIf { it.isNotBlank() } ?: "Android"
    val validBase = HealthUrl.normalise(base)
    val busy = attempt is Attempt.Working || attempt is Attempt.Code

    fun connected(normalised: String, token: String) {
        scope.launch { graph.health.save(HealthCredentials(normalised, token)) }
        attempt = Attempt.Idle
    }

    fun failed(f: HealthFailure) {
        attempt = Attempt.Failed(
            when (f) {
                HealthFailure.SignedOut -> R.string.health_wrong_credentials
                HealthFailure.NotOwner -> R.string.health_not_owner
                is HealthFailure.Unreachable -> R.string.health_unreachable_body
                HealthFailure.Invalid -> R.string.health_invalid
            },
        )
    }

    SectionCard(stringResource(R.string.health_settings_title), accent = Palette.Danger) {
        Text(
            stringResource(R.string.health_settings_body),
            style = MaterialTheme.typography.bodySmall,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
        )
        val creds = credentials
        if (creds != null) {
            Text(stringResource(R.string.health_connected, creds.base, HealthClient.tokenPrefix(creds.token)))
            TextButton(onClick = { disconnecting = true }) {
                Text(stringResource(R.string.health_disconnect), color = MaterialTheme.colorScheme.error)
            }
        } else {
            OutlinedTextField(
                value = base,
                onValueChange = { base = it },
                label = { Text(stringResource(R.string.health_base_url)) },
                placeholder = { Text("https://vmui.example.com") },
                supportingText = {
                    if (base.isNotBlank() && validBase == null) Text(stringResource(R.string.health_base_url_https))
                },
                isError = base.isNotBlank() && validBase == null,
                singleLine = true,
                enabled = !busy,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
                modifier = Modifier.fillMaxWidth(),
            )
            TextButton(
                enabled = validBase != null && !busy,
                onClick = {
                    val normalised = validBase ?: return@TextButton
                    val client = HealthClient.forBase(normalised) ?: return@TextButton
                    attempt = Attempt.Working
                    job = scope.launch {
                        val pending = when (val r = client.pairStart(deviceName)) {
                            is HealthResult.Err -> return@launch failed(r.failure)
                            is HealthResult.Ok -> r.value
                        }
                        attempt = Attempt.Code(pending.code)
                        val deadline = System.currentTimeMillis() + PAIR_MAX_MS
                        while (System.currentTimeMillis() < deadline) {
                            delay(PAIR_POLL_MS)
                            when (val r = client.pairPoll(pending.id)) {
                                // A blip while waiting is not the end: the owner may still approve.
                                is HealthResult.Err -> if (r.failure !is HealthFailure.Unreachable) return@launch failed(r.failure)
                                is HealthResult.Ok -> when (r.value) {
                                    PairState.Approved -> return@launch connected(normalised, pending.token)
                                    PairState.Rejected -> return@launch run { attempt = Attempt.Failed(R.string.health_pair_rejected) }
                                    PairState.Expired -> return@launch run { attempt = Attempt.Failed(R.string.health_pair_expired) }
                                    PairState.SlowDown -> delay(SLOW_DOWN_MS)
                                    PairState.Pending -> Unit
                                }
                            }
                        }
                        attempt = Attempt.Failed(R.string.health_pair_expired)
                    }
                },
            ) { Text(stringResource(R.string.health_pair_code)) }

            Text(stringResource(R.string.health_or_sign_in), style = MaterialTheme.typography.bodySmall)
            OutlinedTextField(
                value = email,
                onValueChange = { email = it },
                label = { Text(stringResource(R.string.health_email)) },
                singleLine = true,
                enabled = !busy,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Email),
                modifier = Modifier.fillMaxWidth(),
            )
            OutlinedTextField(
                value = password,
                onValueChange = { password = it },
                label = { Text(stringResource(R.string.health_password)) },
                singleLine = true,
                enabled = !busy,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                modifier = Modifier.fillMaxWidth(),
            )
            TextButton(
                enabled = validBase != null && email.isNotBlank() && password.isNotEmpty() && !busy,
                onClick = {
                    val normalised = validBase ?: return@TextButton
                    val client = HealthClient.forBase(normalised) ?: return@TextButton
                    val secret = password
                    password = ""
                    attempt = Attempt.Working
                    job = scope.launch {
                        when (val r = client.signIn(deviceName, email, secret)) {
                            is HealthResult.Err -> failed(r.failure)
                            is HealthResult.Ok -> connected(normalised, r.value)
                        }
                    }
                },
            ) { Text(stringResource(R.string.health_sign_in)) }

            val status = attempt
            val line = when (status) {
                Attempt.Idle -> null
                Attempt.Working -> stringResource(R.string.health_connecting)
                is Attempt.Code -> stringResource(R.string.health_pair_waiting)
                is Attempt.Failed -> stringResource(status.message)
            }
            if (status is Attempt.Code && status.code != null) {
                Text(
                    status.code,
                    style = MaterialTheme.typography.displaySmall,
                    fontWeight = FontWeight.Bold,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
            if (line != null) {
                Text(
                    line,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (status is Attempt.Failed) MaterialTheme.colorScheme.error else MaterialTheme.colorScheme.onSurfaceVariant,
                    modifier = Modifier.semantics { liveRegion = LiveRegionMode.Polite },
                )
            }
            if (busy) {
                FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
                    TextButton(onClick = {
                        job?.cancel()
                        job = null
                        attempt = Attempt.Idle
                    }) { Text(stringResource(R.string.cancel)) }
                }
            }
        }
    }

    if (disconnecting) {
        AlertDialog(
            onDismissRequest = { disconnecting = false },
            title = { Text(stringResource(R.string.health_disconnect)) },
            text = { Text(stringResource(R.string.health_disconnect_confirm)) },
            confirmButton = {
                TextButton(onClick = {
                    scope.launch { graph.health.clear() }
                    disconnecting = false
                }) { Text(stringResource(R.string.health_disconnect), color = MaterialTheme.colorScheme.error) }
            },
            dismissButton = { TextButton(onClick = { disconnecting = false }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}
