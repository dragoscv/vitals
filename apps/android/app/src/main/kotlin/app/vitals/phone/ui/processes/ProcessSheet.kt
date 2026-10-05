package app.vitals.phone.ui.processes

import app.vitals.phone.ui.theme.readable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.ui.draw.clip
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.Button
import androidx.compose.material3.ButtonDefaults
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.FilterChip
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.hapticfeedback.HapticFeedbackType
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalHapticFeedback
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.unit.dp
import app.vitals.core.model.ControlRequest
import app.vitals.core.model.Priority
import app.vitals.core.model.Process
import app.vitals.core.model.ProcessState
import app.vitals.core.net.ApiResult
import app.vitals.core.pairing.Pairing
import app.vitals.phone.R
import app.vitals.phone.data.canControl
import app.vitals.phone.data.noteFailure
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalSnackbar
import app.vitals.phone.ui.components.InfoRow
import app.vitals.phone.ui.controlMessage
import app.vitals.phone.ui.theme.glassSurface
import app.vitals.phone.ui.words
import app.vitals.ui.Contrast
import app.vitals.ui.Format
import app.vitals.ui.Palette
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ProcessSheet(pairing: Pairing, process: Process, onDismiss: () -> Unit) {
    val graph = LocalContext.current.graph
    val resources = LocalResources.current
    val snackbar = LocalSnackbar.current
    val haptics = LocalHapticFeedback.current
    val scope = rememberCoroutineScope()
    val sheet = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var busy by remember { mutableStateOf(false) }
    var armed by remember(process.key) { mutableStateOf(false) }
    // The answer is shown inside the sheet: a modal sheet sits in its own
    // window above the scaffold, so a snackbar raised while it is open was
    // drawn underneath it and the tap looked as if it did nothing (found on
    // the A51, 2026-09-28).
    var outcome by remember(process.key) { mutableStateOf<Pair<Boolean, String>?>(null) }

    // The confirm tap must come soon after the first, or the arming lapses:
    // a sheet left open must not be one stray touch from ending a program.
    LaunchedEffect(armed) {
        if (armed) {
            delay(4_000)
            armed = false
        }
    }

    fun run(request: ControlRequest, dismiss: Boolean = false) {
        if (busy) return
        busy = true
        scope.launch {
            val result = graph.client(pairing).control(request)
            busy = false
            when (result) {
                is ApiResult.Ok -> {
                    haptics.performHapticFeedback(HapticFeedbackType.Confirm)
                    val text = resources.getString(R.string.control_done)
                    if (dismiss) {
                        onDismiss()
                        snackbar.showSnackbar(text)
                    } else {
                        outcome = true to text
                    }
                }
                is ApiResult.Err -> {
                    haptics.performHapticFeedback(HapticFeedbackType.Reject)
                    graph.noteFailure(pairing, result.failure)
                    outcome = false to result.failure.controlMessage(resources)
                }
            }
        }
    }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheet,
        containerColor = Color.Transparent,
        // Stated, not derived: a transparent container yields black text,
        // unreadable on the dark glass behind it.
        contentColor = MaterialTheme.colorScheme.onSurface,
        dragHandle = null,
    ) {
        Column(
            Modifier
                .fillMaxWidth()
                .clip(RoundedCornerShape(topStart = 28.dp, topEnd = 28.dp))
                .glassSurface()
                .verticalScroll(rememberScrollState())
                .padding(20.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Text(process.displayName, style = MaterialTheme.typography.headlineSmall)
            InfoRow(stringResource(R.string.sort_cpu), Format.percent(process.cpu))
            InfoRow(stringResource(R.string.metric_memory), Format.bytes(process.memoryPrivate))
            InfoRow(
                stringResource(R.string.process_disk),
                stringResource(R.string.disk_read_write, Format.rate(process.diskRead), Format.rate(process.diskWrite)),
            )
            InfoRow(stringResource(R.string.metric_gpu), Format.percent(process.gpu))
            InfoRow(stringResource(R.string.process_threads), process.threadCount.toString())
            InfoRow(stringResource(R.string.process_uptime), Format.duration(process.uptimeSecs))
            InfoRow(stringResource(R.string.process_user), process.user ?: Format.DASH)
            InfoRow(stringResource(R.string.process_pid), process.key.pid.toString())

            when {
                !pairing.canControl() -> Text(
                    stringResource(R.string.process_read_only),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                !process.isSafelyTerminable -> Text(
                    stringResource(R.string.process_protected),
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
                else -> Actions(process, busy, armed, onArm = {
                    haptics.performHapticFeedback(HapticFeedbackType.LongPress)
                    armed = true
                }, run = ::run)
            }
            outcome?.let { (ok, text) ->
                Text(
                    text,
                    style = MaterialTheme.typography.bodyMedium,
                    color = if (ok) readable(Palette.Ok) else MaterialTheme.colorScheme.error,
                )
            }
        }
    }
}

@Composable
private fun Actions(
    process: Process,
    busy: Boolean,
    armed: Boolean,
    onArm: () -> Unit,
    run: (ControlRequest, Boolean) -> Unit,
) {
    Column(verticalArrangement = Arrangement.spacedBy(12.dp)) {
        Row(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Button(
                onClick = { if (armed) run(ControlRequest.Terminate(process.key), true) else onArm() },
                enabled = !busy,
                // Darkened until white reads at 4.5:1: the raw danger red gave 4.05:1.
                colors = ButtonDefaults.buttonColors(containerColor = Contrast.readable(Palette.Danger, Color.White), contentColor = Color.White),
                modifier = Modifier.weight(1f),
            ) { Text(stringResource(if (armed) R.string.action_end_confirm else R.string.action_end)) }
            val suspended = process.state == ProcessState.Suspended
            OutlinedButton(
                onClick = {
                    run(
                        if (suspended) ControlRequest.Resume(process.key) else ControlRequest.Suspend(process.key),
                        false,
                    )
                },
                enabled = !busy,
                modifier = Modifier.weight(1f),
            ) { Text(stringResource(if (suspended) R.string.action_resume else R.string.action_suspend)) }
        }
        Text(stringResource(R.string.action_priority), style = MaterialTheme.typography.labelLarge)
        FlowRow(horizontalArrangement = Arrangement.spacedBy(8.dp)) {
            Priority.entries.forEach { p ->
                // The wire does not report a process's current priority, so
                // no chip is shown as selected; each tap is a fresh request.
                FilterChip(
                    selected = false,
                    onClick = { run(ControlRequest.SetPriority(process.key, p), false) },
                    enabled = !busy,
                    label = { Text(stringResource(p.words())) },
                )
            }
        }
        Row(verticalAlignment = Alignment.CenterVertically) {
            Column(Modifier.weight(1f)) {
                Text(stringResource(R.string.action_efficiency), style = MaterialTheme.typography.bodyLarge)
                Text(
                    stringResource(R.string.action_efficiency_body),
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Switch(
                checked = process.inEfficiencyMode,
                onCheckedChange = { run(ControlRequest.SetEfficiencyMode(process.key, it), false) },
                enabled = !busy,
            )
        }
    }
}
