package app.vitals.tv.ui.add

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.border
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.BasicTextField
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.focus.onFocusChanged
import androidx.compose.ui.graphics.SolidColor
import androidx.compose.ui.input.key.Key
import androidx.compose.ui.input.key.KeyEventType
import androidx.compose.ui.input.key.key
import androidx.compose.ui.input.key.onPreviewKeyEvent
import androidx.compose.ui.input.key.type
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.style.TextAlign
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import androidx.tv.material3.ClickableSurfaceDefaults
import androidx.tv.material3.ListItem
import androidx.tv.material3.MaterialTheme
import androidx.tv.material3.Surface
import androidx.tv.material3.Text
import app.vitals.core.net.DiscoveredPc
import app.vitals.core.net.Discovery
import app.vitals.core.net.LanAddress
import app.vitals.core.pairing.PairCheck
import app.vitals.core.pairing.PairCheckResult
import app.vitals.tv.LocalNavigator
import app.vitals.tv.R
import app.vitals.tv.Route
import app.vitals.tv.data.tvGraph
import app.vitals.tv.ui.Action
import app.vitals.tv.ui.Hint
import app.vitals.tv.ui.Panel
import app.vitals.tv.ui.SafeHorizontal
import app.vitals.tv.ui.SafeVertical
import app.vitals.tv.ui.ScreenTitle
import app.vitals.tv.ui.message
import kotlinx.coroutines.launch

/** Where the pairing is. A PC is picked first; the secret is typed second. */
private sealed interface Step {
    data object Pick : Step
    data class Address(val prefill: String) : Step
    data class Code(val baseUrl: String, val name: String?) : Step
    data class Token(val baseUrl: String, val name: String?) : Step
}

/**
 * Pairing without a camera. The PC shows a six-digit code for five minutes
 * (Settings, Remote access, Pair a TV); the TV finds it by mDNS and trades
 * the code for a token over `POST /api/v1/pair`. Six digits is what a remote
 * can type: the number keys work directly, and the on-screen keypad works
 * on remotes that have none. A pasted token stays as the fallback, for a PC
 * running an older Vitals without the code route.
 */
@Composable
fun AddPcScreen() {
    var step by remember { mutableStateOf<Step>(Step.Pick) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }
    val graph = LocalContext.current.tvGraph
    val resources = LocalResources.current
    val navigator = LocalNavigator.current
    val scope = rememberCoroutineScope()
    val tvLabel = stringResource(R.string.tv_label)

    fun finish(result: PairCheckResult) {
        when (result) {
            is PairCheckResult.Ok -> scope.launch {
                graph.save(result.pairing)
                navigator.top(Route.Overview)
                navigator.push(Route.Pc(result.pairing.id))
            }
            is PairCheckResult.Refused -> error = if (result.status != null) {
                resources.getString(result.reason.message(), result.status)
            } else {
                resources.getString(result.reason.message())
            }
        }
    }

    Column(Modifier.fillMaxSize().padding(horizontal = SafeHorizontal, vertical = SafeVertical), verticalArrangement = Arrangement.spacedBy(16.dp)) {
        ScreenTitle(stringResource(R.string.add_title))
        error?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.bodyLarge) }
        if (busy) Hint(stringResource(R.string.pairing_checking))
        when (val s = step) {
            Step.Pick -> PickStep(
                onPc = { pc -> error = null; step = Step.Code(pc.baseUrl, pc.name) },
                onManual = { error = null; step = Step.Address("") },
            )
            is Step.Address -> AddressStep(s.prefill) { url -> error = null; step = Step.Code(url, null) }
            is Step.Code -> CodeStep(
                name = s.name ?: s.baseUrl,
                busy = busy,
                onCode = { code ->
                    busy = true
                    error = null
                    scope.launch {
                        finish(PairCheck.withCode(s.baseUrl, code, tvLabel, s.name, graph.pairings.load()))
                        busy = false
                    }
                },
                onToken = { error = null; step = Step.Token(s.baseUrl, s.name) },
            )
            is Step.Token -> TokenStep(busy) { token ->
                busy = true
                error = null
                scope.launch {
                    finish(PairCheck.withToken(s.baseUrl, token, s.name, graph.pairings.load()))
                    busy = false
                }
            }
        }
    }
}

@Composable
private fun PickStep(onPc: (DiscoveredPc) -> Unit, onManual: () -> Unit) {
    val context = LocalContext.current
    val discovery = remember { Discovery(context) }
    val found by remember { discovery.scan() }.collectAsStateWithLifecycle(emptyList())
    // Only private addresses are offered; a spoofed mDNS answer pointing at
    // the internet never reaches the code prompt.
    val safe = remember(found) { found.filter { LanAddress.isPrivate(it.baseUrl) } }
    Row(horizontalArrangement = Arrangement.spacedBy(32.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(12.dp)) {
            Panel(stringResource(R.string.add_title)) { Text(stringResource(R.string.add_how), style = MaterialTheme.typography.bodyLarge) }
            Action(stringResource(R.string.add_manual), onClick = onManual)
        }
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(8.dp)) {
            Text(stringResource(R.string.add_nearby), style = MaterialTheme.typography.titleLarge)
            Hint(stringResource(if (safe.isEmpty()) R.string.add_none_found else R.string.add_searching))
            LazyColumn(verticalArrangement = Arrangement.spacedBy(4.dp), contentPadding = PaddingValues(bottom = 24.dp)) {
                items(safe, key = { it.name }) { pc ->
                    ListItem(
                        selected = false,
                        onClick = { onPc(pc) },
                        headlineContent = { Text(pc.name) },
                        supportingContent = { Text(listOfNotNull(pc.version?.let { "Vitals $it" }, pc.host).joinToString(" · ")) },
                    )
                }
            }
        }
    }
}

@Composable
private fun AddressStep(prefill: String, onNext: (String) -> Unit) {
    var url by remember { mutableStateOf(prefill) }
    val focus = remember { FocusRequester() }
    // A new step replaces the focused control; without this, focus falls
    // back to the drawer and the user has to find the field again.
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth(0.6f)) {
        Field(stringResource(R.string.field_address), url, { url = it }, stringResource(R.string.field_address_hint), KeyboardType.Uri, focus) {
            if (url.isNotBlank()) PairCheck.normalise(url)?.let(onNext)
        }
        Action(stringResource(R.string.next), onClick = { PairCheck.normalise(url)?.let(onNext) }, enabled = url.isNotBlank())
    }
}

/**
 * Six boxes and a keypad. Digits typed on the remote's number keys land here
 * directly; the keypad is for remotes without them (the Chromecast's has
 * none). The code is sent as soon as the sixth digit arrives.
 */
@Composable
private fun CodeStep(name: String, busy: Boolean, onCode: (String) -> Unit, onToken: () -> Unit) {
    var code by remember { mutableStateOf("") }
    val first = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { first.requestFocus() } }

    fun type(d: Char) {
        if (busy || code.length >= 6) return
        code += d
        if (code.length == 6) onCode(code)
    }

    Row(horizontalArrangement = Arrangement.spacedBy(48.dp)) {
        Column(Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(16.dp)) {
            Text(stringResource(R.string.add_code_title, name), style = MaterialTheme.typography.titleLarge)
            Hint(stringResource(R.string.add_code_body))
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                repeat(6) { i ->
                    Surface(
                        shape = RoundedCornerShape(12.dp),
                        modifier = Modifier.size(width = 64.dp, height = 80.dp),
                    ) {
                        Text(
                            code.getOrNull(i)?.toString() ?: "",
                            modifier = Modifier.fillMaxSize().padding(top = 16.dp),
                            textAlign = TextAlign.Center,
                            fontSize = 40.sp,
                            fontWeight = FontWeight.SemiBold,
                        )
                    }
                    if (i == 2) androidx.compose.foundation.layout.Spacer(Modifier.size(12.dp))
                }
            }
            Row(horizontalArrangement = Arrangement.spacedBy(12.dp)) {
                Action(stringResource(R.string.add_code_delete), onClick = { if (!busy) code = code.dropLast(1) })
                Action(stringResource(R.string.add_use_token), onClick = onToken)
            }
        }
        Column(
            verticalArrangement = Arrangement.spacedBy(10.dp),
            modifier = Modifier.onPreviewKeyEvent { e ->
                // The remote's number keys, wherever focus is on the keypad.
                if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                val digit = digitOf(e.key) ?: return@onPreviewKeyEvent false
                type(digit)
                true
            },
        ) {
            listOf("123", "456", "789", "0").forEachIndexed { r, row ->
                Row(horizontalArrangement = Arrangement.spacedBy(10.dp), modifier = Modifier.align(Alignment.CenterHorizontally)) {
                    row.forEachIndexed { c, d ->
                        Surface(
                            onClick = { type(d) },
                            enabled = !busy,
                            shape = ClickableSurfaceDefaults.shape(RoundedCornerShape(16.dp)),
                            modifier = Modifier.size(84.dp).let { if (r == 0 && c == 0) it.focusRequester(first) else it },
                        ) {
                            Text(
                                d.toString(),
                                modifier = Modifier.fillMaxSize().padding(top = 18.dp),
                                textAlign = TextAlign.Center,
                                fontSize = 36.sp,
                            )
                        }
                    }
                }
            }
        }
    }
}

private fun digitOf(key: Key): Char? = when (key) {
    Key.Zero, Key.NumPad0 -> '0'
    Key.One, Key.NumPad1 -> '1'
    Key.Two, Key.NumPad2 -> '2'
    Key.Three, Key.NumPad3 -> '3'
    Key.Four, Key.NumPad4 -> '4'
    Key.Five, Key.NumPad5 -> '5'
    Key.Six, Key.NumPad6 -> '6'
    Key.Seven, Key.NumPad7 -> '7'
    Key.Eight, Key.NumPad8 -> '8'
    Key.Nine, Key.NumPad9 -> '9'
    else -> null
}

@Composable
private fun TokenStep(busy: Boolean, onToken: (String) -> Unit) {
    var token by remember { mutableStateOf("") }
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { runCatching { focus.requestFocus() } }
    Column(verticalArrangement = Arrangement.spacedBy(12.dp), modifier = Modifier.fillMaxWidth(0.7f)) {
        Hint(stringResource(R.string.add_token_body))
        Field(stringResource(R.string.field_token), token, { token = it.trim() }, "", KeyboardType.Password, focus) {
            if (token.isNotBlank()) onToken(token)
        }
        Action(stringResource(R.string.pair), onClick = { onToken(token) }, enabled = !busy && token.isNotBlank())
    }
}

/**
 * A text field for the TV. tv-material has no text field of its own, so this
 * wraps [BasicTextField]; the border turns white while it has focus, because
 * a cursor alone is invisible from the sofa. OK opens the system keyboard.
 */
@Composable
private fun Field(
    label: String,
    value: String,
    onChange: (String) -> Unit,
    placeholder: String,
    type: KeyboardType,
    focus: FocusRequester,
    onDone: () -> Unit,
) {
    var focused by remember { mutableStateOf(false) }
    val shape = RoundedCornerShape(12.dp)
    val manager = androidx.compose.ui.platform.LocalFocusManager.current
    Column(verticalArrangement = Arrangement.spacedBy(6.dp)) {
        Text(label, style = MaterialTheme.typography.titleSmall)
        Surface(
            shape = shape,
            modifier = Modifier.fillMaxWidth().border(
                androidx.compose.foundation.BorderStroke(if (focused) 3.dp else 1.dp, if (focused) androidx.compose.ui.graphics.Color.White else MaterialTheme.colorScheme.border),
                shape,
            ),
        ) {
            BasicTextField(
                value = value,
                onValueChange = onChange,
                singleLine = true,
                textStyle = MaterialTheme.typography.titleMedium.copy(color = MaterialTheme.colorScheme.onSurface),
                cursorBrush = SolidColor(MaterialTheme.colorScheme.primary),
                keyboardOptions = KeyboardOptions(keyboardType = type, imeAction = ImeAction.Done),
                keyboardActions = KeyboardActions(onDone = { onDone() }),
                modifier = Modifier
                    .fillMaxWidth()
                    .padding(16.dp)
                    .focusRequester(focus)
                    .onFocusChanged { focused = it.isFocused }
                    // A single-line field has nowhere to move the cursor up or
                    // down, yet it swallows those keys, which trapped the D-pad
                    // on the field with Next unreachable below it.
                    .onPreviewKeyEvent { e ->
                        if (e.type != KeyEventType.KeyDown) return@onPreviewKeyEvent false
                        when (e.key) {
                            Key.DirectionDown -> manager.moveFocus(androidx.compose.ui.focus.FocusDirection.Down)
                            Key.DirectionUp -> manager.moveFocus(androidx.compose.ui.focus.FocusDirection.Up)
                            else -> false
                        }
                    },
                decorationBox = { inner ->
                    if (value.isEmpty() && placeholder.isNotEmpty()) Hint(placeholder)
                    inner()
                },
            )
        }
    }
}
