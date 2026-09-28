package app.vitals.phone.ui.add

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.AnimatedContent
import androidx.compose.animation.fadeIn
import androidx.compose.animation.fadeOut
import androidx.compose.animation.togetherWith
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.aspectRatio
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.Button
import androidx.compose.material3.LinearProgressIndicator
import androidx.compose.material3.ListItem
import androidx.compose.material3.ListItemDefaults
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.PrimaryTabRow
import androidx.compose.material3.Tab
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.platform.LocalResources
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import app.vitals.core.net.DiscoveredPc
import app.vitals.core.net.Discovery
import app.vitals.core.net.LanAddress
import app.vitals.core.pairing.PairingLink
import app.vitals.phone.R
import app.vitals.phone.graph
import app.vitals.phone.ui.LocalNavigator
import app.vitals.phone.ui.LocalSnackbar
import app.vitals.phone.ui.components.GlassScaffold
import kotlinx.coroutines.launch

private enum class Mode { Scan, Nearby, Manual }

@Composable
fun AddPcScreen(startWithScan: Boolean) {
    val context = LocalContext.current
    val graph = context.graph
    val resources = LocalResources.current
    val navigator = LocalNavigator.current
    val snackbar = LocalSnackbar.current
    val scope = rememberCoroutineScope()
    // A QR scan is the fastest path, so it is the default wherever there is
    // a camera; without one, a nearby PC is the next easiest.
    val hasCamera = remember { context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY) }
    var mode by rememberSaveable { mutableStateOf(if (startWithScan || hasCamera) Mode.Scan else Mode.Nearby) }
    var busy by remember { mutableStateOf(false) }
    var error by remember { mutableStateOf<String?>(null) }

    fun submit(url: String, token: String, label: String?) {
        if (busy) return
        busy = true
        error = null
        scope.launch {
            when (val out = Pairer.pair(graph, url, token, label)) {
                is PairOutcome.Saved -> {
                    snackbar.showSnackbar(resources.getString(R.string.pairing_done, out.pairing.label))
                    navigator.pop()
                }
                is PairOutcome.Refused -> error = if (out.status != null) {
                    resources.getString(out.message, out.status)
                } else {
                    resources.getString(out.message)
                }
            }
            busy = false
        }
    }

    GlassScaffold(title = stringResource(R.string.add_title)) { padding ->
        Column(Modifier.fillMaxSize().padding(top = padding.calculateTopPadding())) {
            PrimaryTabRow(
                selectedTabIndex = mode.ordinal,
                containerColor = Color.Transparent,
                contentColor = MaterialTheme.colorScheme.onSurface,
            ) {
                Mode.entries.forEach { m ->
                    Tab(
                        selected = mode == m,
                        onClick = { mode = m; error = null },
                        text = {
                            Text(
                                stringResource(
                                    when (m) {
                                        Mode.Scan -> R.string.add_scan
                                        Mode.Nearby -> R.string.add_nearby
                                        Mode.Manual -> R.string.add_manual
                                    },
                                ),
                            )
                        },
                    )
                }
            }
            if (busy) LinearProgressIndicator(Modifier.fillMaxWidth())
            error?.let {
                Text(
                    it,
                    color = MaterialTheme.colorScheme.error,
                    modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
                )
            }
            val inner = PaddingValues(start = 16.dp, end = 16.dp, top = 12.dp, bottom = padding.calculateBottomPadding() + 24.dp)
            AnimatedContent(mode, transitionSpec = { fadeIn() togetherWith fadeOut() }, label = "mode") { m ->
                when (m) {
                    Mode.Scan -> ScanTab(inner, busy) { link -> submit(link.baseUrl, link.token, null) }
                    Mode.Nearby -> NearbyTab(inner, busy) { pc, token -> submit(pc.baseUrl, token, pc.name) }
                    Mode.Manual -> ManualTab(inner, busy) { url, token, name -> submit(url, token, name) }
                }
            }
        }
    }
}

@Composable
private fun ScanTab(padding: PaddingValues, busy: Boolean, onLink: (PairingLink) -> Unit) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var denied by remember { mutableStateOf(false) }
    var notVitals by remember { mutableStateOf(false) }
    var lastCode by remember { mutableStateOf<String?>(null) }
    val launcher = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        granted = ok
        denied = !ok
    }

    Column(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        if (granted) {
            Box(
                Modifier.fillMaxWidth().aspectRatio(1f).clip(RoundedCornerShape(28.dp)),
            ) {
                QrScanner(
                    onCode = { code ->
                        // The analyser reports the same code on every frame
                        // while it is in view; act on it once.
                        if (busy || code == lastCode) return@QrScanner
                        lastCode = code
                        val link = PairingLink.parse(code)
                        notVitals = link == null
                        if (link != null) onLink(link)
                    },
                    modifier = Modifier.fillMaxSize(),
                )
            }
            Text(stringResource(R.string.add_scan_hint), style = MaterialTheme.typography.bodyMedium)
            if (notVitals) Text(stringResource(R.string.scan_not_vitals), color = MaterialTheme.colorScheme.error)
        } else {
            Text(stringResource(if (denied) R.string.camera_denied else R.string.camera_rationale))
            Button(onClick = { launcher.launch(Manifest.permission.CAMERA) }) {
                Text(stringResource(R.string.camera_allow))
            }
        }
    }
}

@Composable
private fun NearbyTab(padding: PaddingValues, busy: Boolean, onPair: (DiscoveredPc, String) -> Unit) {
    val context = LocalContext.current
    val discovery = remember { Discovery(context) }
    val found by remember { discovery.scan() }.collectAsStateWithLifecycle(emptyList())
    var chosen by remember { mutableStateOf<DiscoveredPc?>(null) }
    // Only private addresses are offered; a spoofed mDNS answer pointing at
    // the internet never reaches the token prompt.
    val safe = remember(found) { found.filter { LanAddress.isPrivate(it.baseUrl) } }

    LazyColumn(contentPadding = padding, verticalArrangement = Arrangement.spacedBy(8.dp), modifier = Modifier.fillMaxSize()) {
        item {
            Text(
                stringResource(if (safe.isEmpty()) R.string.nearby_empty else R.string.nearby_searching),
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
        }
        items(safe, key = { it.name }) { pc ->
            ListItem(
                headlineContent = { Text(pc.name) },
                supportingContent = {
                    Text(listOfNotNull(pc.version?.let { stringResource(R.string.nearby_version, it) }, pc.host).joinToString(" · "))
                },
                colors = ListItemDefaults.colors(containerColor = MaterialTheme.colorScheme.surfaceContainer.copy(alpha = 0.92f)),
                modifier = Modifier.clip(RoundedCornerShape(20.dp)).clickable(enabled = !busy) { chosen = pc }.animateItem(),
            )
        }
    }

    chosen?.let { pc ->
        var token by remember(pc) { mutableStateOf("") }
        AlertDialog(
            onDismissRequest = { chosen = null },
            title = { Text(stringResource(R.string.nearby_token_title, pc.name)) },
            text = {
                Column(verticalArrangement = Arrangement.spacedBy(8.dp)) {
                    Text(stringResource(R.string.nearby_token_body))
                    OutlinedTextField(
                        value = token,
                        onValueChange = { token = it.trim() },
                        label = { Text(stringResource(R.string.field_token)) },
                        singleLine = true,
                        visualTransformation = PasswordVisualTransformation(),
                        keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                    )
                }
            },
            confirmButton = {
                TextButton(onClick = { onPair(pc, token); chosen = null }, enabled = token.isNotEmpty()) {
                    Text(stringResource(R.string.pair))
                }
            },
            dismissButton = { TextButton(onClick = { chosen = null }) { Text(stringResource(R.string.cancel)) } },
        )
    }
}

@Composable
private fun ManualTab(padding: PaddingValues, busy: Boolean, onPair: (String, String, String?) -> Unit) {
    var url by rememberSaveable { mutableStateOf("") }
    var token by remember { mutableStateOf("") }
    var name by rememberSaveable { mutableStateOf("") }
    Column(Modifier.fillMaxSize().padding(padding), verticalArrangement = Arrangement.spacedBy(12.dp)) {
        OutlinedTextField(
            value = url,
            onValueChange = { url = it },
            label = { Text(stringResource(R.string.field_address)) },
            placeholder = { Text(stringResource(R.string.field_address_hint)) },
            singleLine = true,
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Uri),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = token,
            onValueChange = { token = it.trim() },
            label = { Text(stringResource(R.string.field_token)) },
            singleLine = true,
            visualTransformation = PasswordVisualTransformation(),
            keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
            modifier = Modifier.fillMaxWidth(),
        )
        OutlinedTextField(
            value = name,
            onValueChange = { name = it },
            label = { Text(stringResource(R.string.field_name)) },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Button(
            onClick = { onPair(url, token, name.ifBlank { null }) },
            enabled = !busy && url.isNotBlank() && token.isNotBlank(),
            modifier = Modifier.fillMaxWidth(),
        ) { Text(stringResource(if (busy) R.string.pairing_checking else R.string.pair)) }
    }
}
