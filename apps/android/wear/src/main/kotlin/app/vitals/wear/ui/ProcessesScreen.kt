package app.vitals.wear.ui

import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.text.style.TextOverflow
import androidx.wear.compose.foundation.lazy.TransformingLazyColumn
import androidx.wear.compose.foundation.lazy.items
import androidx.wear.compose.foundation.lazy.rememberTransformingLazyColumnState
import androidx.wear.compose.material3.AlertDialog
import androidx.wear.compose.material3.AlertDialogDefaults
import androidx.wear.compose.material3.Button
import androidx.wear.compose.material3.ButtonDefaults
import androidx.wear.compose.material3.CircularProgressIndicator
import androidx.wear.compose.material3.FailureConfirmationDialog
import androidx.wear.compose.material3.ListHeader
import androidx.wear.compose.material3.MaterialTheme
import androidx.wear.compose.material3.ScreenScaffold
import androidx.wear.compose.material3.SuccessConfirmationDialog
import androidx.wear.compose.material3.SurfaceTransformation
import androidx.wear.compose.material3.Text
import androidx.wear.compose.material3.curvedText
import androidx.wear.compose.material3.lazy.rememberTransformationSpec
import androidx.wear.compose.material3.lazy.transformedHeight
import app.vitals.core.model.Process
import app.vitals.ui.Format
import app.vitals.wear.R
import app.vitals.wear.data.ControlOutcome
import kotlinx.coroutines.launch

private sealed interface EndTaskUi {
    data object Idle : EndTaskUi
    data class Confirm(val process: Process) : EndTaskUi
    data class Working(val process: Process) : EndTaskUi
    data class Result(val process: Process, val outcome: ControlOutcome) : EndTaskUi
}

@Composable
fun ProcessesScreen(
    processes: List<Process>,
    canControl: Boolean,
    onEndTask: suspend (Process) -> ControlOutcome,
) {
    val listState = rememberTransformingLazyColumnState()
    val spec = rememberTransformationSpec()
    val scope = rememberCoroutineScope()
    var ui by remember { mutableStateOf<EndTaskUi>(EndTaskUi.Idle) }

    ScreenScaffold(scrollState = listState) { padding ->
        TransformingLazyColumn(state = listState, contentPadding = padding) {
            item {
                ListHeader(modifier = Modifier.transformedHeight(this, spec), transformation = SurfaceTransformation(spec)) {
                    Text(stringResource(R.string.top_processes))
                }
            }
            if (processes.isEmpty()) {
                item {
                    Message(
                        title = stringResource(R.string.no_processes_title),
                        body = stringResource(R.string.no_processes_body),
                        modifier = Modifier.transformedHeight(this, spec),
                    )
                }
            }
            items(processes, key = { "${it.key.pid}:${it.key.startTime}" }) { process ->
                // A protected or critical process is shown but cannot be tapped:
                // the PC would refuse, and offering it would only teach distrust.
                val tappable = canControl && process.isSafelyTerminable
                Button(
                    onClick = { if (tappable) ui = EndTaskUi.Confirm(process) },
                    enabled = tappable || !canControl,
                    modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
                    transformation = SurfaceTransformation(spec),
                    colors = ButtonDefaults.filledTonalButtonColors(),
                    label = { Text(process.name, maxLines = 1, overflow = TextOverflow.Ellipsis) },
                    secondaryLabel = {
                        Text(
                            stringResource(
                                R.string.process_line,
                                Format.percent(process.cpu),
                                Format.bytes(process.memoryPrivate),
                            ),
                        )
                    },
                )
            }
            if (!canControl && processes.isNotEmpty()) {
                item {
                    Text(
                        stringResource(R.string.read_only_note),
                        modifier = Modifier.fillMaxWidth().transformedHeight(this, spec),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        textAlign = TextAlign.Center,
                    )
                }
            }
        }
    }

    val confirm = ui as? EndTaskUi.Confirm
    AlertDialog(
        visible = confirm != null,
        onDismissRequest = { ui = EndTaskUi.Idle },
        title = { Text(stringResource(R.string.end_task_title, confirm?.process?.name.orEmpty()), textAlign = TextAlign.Center) },
        text = { Text(stringResource(R.string.end_task_body), textAlign = TextAlign.Center) },
        confirmButton = {
            AlertDialogDefaults.ConfirmButton(onClick = {
                val target = confirm?.process ?: return@ConfirmButton
                ui = EndTaskUi.Working(target)
                scope.launch { ui = EndTaskUi.Result(target, onEndTask(target)) }
            })
        },
        dismissButton = { AlertDialogDefaults.DismissButton(onClick = { ui = EndTaskUi.Idle }) },
    )

    val working = ui as? EndTaskUi.Working
    AlertDialog(
        visible = working != null,
        // A request in flight cannot be taken back; dismissing only hides it.
        onDismissRequest = {},
        title = { Text(stringResource(R.string.ending, working?.process?.name.orEmpty()), textAlign = TextAlign.Center) },
        icon = { CircularProgressIndicator() },
    )

    val result = ui as? EndTaskUi.Result
    val successText = stringResource(R.string.end_task_done)
    SuccessConfirmationDialog(
        visible = result?.outcome == ControlOutcome.Done,
        onDismissRequest = { ui = EndTaskUi.Idle },
        curvedText = { curvedText(successText) },
    )
    val failure = (result?.outcome as? ControlOutcome.Failed)?.reason
    val failureText = when (failure) {
        ControlOutcome.Reason.ReadOnly -> stringResource(R.string.fail_read_only)
        ControlOutcome.Reason.AccessDenied -> stringResource(R.string.fail_access_denied)
        ControlOutcome.Reason.AlreadyGone -> stringResource(R.string.fail_gone)
        ControlOutcome.Reason.Unreachable -> stringResource(R.string.fail_unreachable)
        ControlOutcome.Reason.Unknown, null -> stringResource(R.string.fail_unknown)
    }
    FailureConfirmationDialog(
        visible = failure != null,
        onDismissRequest = { ui = EndTaskUi.Idle },
        curvedText = { curvedText(failureText) },
    )
}
