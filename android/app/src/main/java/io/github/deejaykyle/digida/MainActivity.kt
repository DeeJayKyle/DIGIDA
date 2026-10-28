package io.github.deejaykyle.digida

import android.Manifest
import android.content.pm.PackageManager
import android.os.Build
import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.animation.Crossfade
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.TextStyle
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.core.content.ContextCompat
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.deejaykyle.digida.ui.App
import io.github.deejaykyle.digida.ui.Backdrop
import io.github.deejaykyle.digida.ui.DigidaTheme
import io.github.deejaykyle.digida.ui.Label
import io.github.deejaykyle.digida.ui.LocalPalette
import io.github.deejaykyle.digida.ui.Pressable

class MainActivity : ComponentActivity() {
    override fun onCreate(savedInstanceState: Bundle?) {
        enableEdgeToEdge()
        super.onCreate(savedInstanceState)
        val app = application as DigidaApp
        app.library.offerLink(sharedLink(intent))

        setContent {
            val finish by app.preferences.finish.collectAsStateWithLifecycle()
            val mode by app.preferences.mode.collectAsStateWithLifecycle()
            DigidaTheme(finish, mode) {
                var allowed by remember { mutableStateOf(hasMusicAccess()) }
                Crossfade(allowed, label = "access") { granted ->
                    when (granted) {
                        true -> App(app.library, app.preferences)
                        false -> AccessScreen { allowed = it }
                    }
                }
            }
        }
    }

    override fun onNewIntent(intent: android.content.Intent) {
        super.onNewIntent(intent)
        (application as DigidaApp).library.offerLink(sharedLink(intent))
    }

    private fun hasMusicAccess() =
        ContextCompat.checkSelfPermission(this, musicPermission()) == PackageManager.PERMISSION_GRANTED
}

/** The link an intent hands the app: one it was asked to open, or the first in shared text. */
private fun sharedLink(intent: android.content.Intent?): String? = when (intent?.action) {
    android.content.Intent.ACTION_VIEW -> intent.dataString
    android.content.Intent.ACTION_SEND ->
        intent.getStringExtra(android.content.Intent.EXTRA_TEXT)
            ?.let { Regex("https?://\\S+").find(it)?.value }
    else -> null
}

/** The permission that reads audio files: its own since Android 13, all storage before. */
private fun musicPermission() = when {
    Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU -> Manifest.permission.READ_MEDIA_AUDIO
    else -> Manifest.permission.READ_EXTERNAL_STORAGE
}

/** Asks for the music, saying why, before anything else is drawn. */
@Composable
private fun AccessScreen(onAnswer: (Boolean) -> Unit) {
    val palette = LocalPalette.current
    val wanted = buildList {
        add(musicPermission())
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            add(Manifest.permission.READ_MEDIA_IMAGES)
            add(Manifest.permission.POST_NOTIFICATIONS)
        }
        // saved files land in Music, which Android 10 and older only let an app write to with this
        if (Build.VERSION.SDK_INT <= Build.VERSION_CODES.Q) add(Manifest.permission.WRITE_EXTERNAL_STORAGE)
    }
    val ask = rememberLauncherForActivityResult(ActivityResultContracts.RequestMultiplePermissions()) { answers ->
        onAnswer(answers[musicPermission()] == true)
    }

    Box(Modifier.fillMaxSize()) {
        Backdrop(palette)
        Column(
            Modifier.fillMaxSize().padding(36.dp),
            verticalArrangement = Arrangement.Center,
            horizontalAlignment = Alignment.CenterHorizontally,
        ) {
            Label("DIGIDA", size = 40.sp, weight = FontWeight.Black)
            Spacer(Modifier.height(28.dp))
            Label(stringResource(R.string.permission_title), size = 20.sp, weight = FontWeight.SemiBold, lines = 2)
            Spacer(Modifier.height(8.dp))
            Label(stringResource(R.string.permission_detail), subtle = true, lines = 3)
            Spacer(Modifier.height(28.dp))
            Pressable(
                onClick = { ask.launch(wanted.toTypedArray()) },
                modifier = Modifier.height(52.dp).width(220.dp),
                fill = palette.accent,
                radius = 26.dp,
            ) {
                Text(
                    stringResource(R.string.permission_grant),
                    style = TextStyle(color = palette.onAccent, fontWeight = FontWeight.SemiBold, fontSize = 16.sp),
                )
            }
        }
    }
}
