package io.github.deejaykyle.digida.ui

import androidx.compose.animation.core.animateFloatAsState
import androidx.compose.foundation.background
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.rounded.Close
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.res.stringResource
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp
import androidx.lifecycle.compose.collectAsStateWithLifecycle
import io.github.deejaykyle.digida.Library
import io.github.deejaykyle.digida.R
import uniffi.digida_mobile.Download
import uniffi.digida_mobile.DownloadState

/**
 * Every download: what is arriving with its progress, what waits, what failed and why, and what
 * is kept. A kept song plays from here, and each one can be removed.
 */
@Composable
fun DownloadsScreen(library: Library, onBack: () -> Unit) {
    val downloads by library.downloads.collectAsStateWithLifecycle()
    val status by library.status.collectAsStateWithLifecycle()
    val kept = downloads.filter { it.state is DownloadState.Done }.map { it.song }
    Column(Modifier.fillMaxSize()) {
        TopBar(stringResource(R.string.downloads), onBack = onBack) {
            if (downloads.any { it.state is DownloadState.Failed }) {
                SmallButton(stringResource(R.string.clear_failed)) { library.clearFailedDownloads() }
            }
        }
        if (downloads.isEmpty()) {
            Label(stringResource(R.string.downloads_empty), Modifier.padding(18.dp), subtle = true, lines = 4)
        }
        LazyColumn(contentPadding = PaddingValues(start = 18.dp, end = 18.dp, bottom = 120.dp)) {
            items(downloads, key = { "${it.song.provider}/${it.song.id}" }) { download ->
                DownloadRow(
                    download,
                    playing = status.song?.id == download.song.id,
                    onPlay = { library.play(kept, kept.indexOf(download.song).coerceAtLeast(0)) },
                    onRemove = { library.removeDownload(download.song) },
                )
            }
        }
    }
}

@Composable
private fun DownloadRow(download: Download, playing: Boolean, onPlay: () -> Unit, onRemove: () -> Unit) {
    val palette = LocalPalette.current
    val state = download.state
    Pressable(
        onClick = { if (state is DownloadState.Done) onPlay() },
        modifier = Modifier.fillMaxWidth().padding(vertical = 4.dp),
        selected = playing,
        contentAlignment = Alignment.CenterStart,
    ) {
        Row(Modifier.padding(10.dp), verticalAlignment = Alignment.CenterVertically) {
            Cover(download.song.cover, download.song.album, Modifier.size(48.dp), radius = 8.dp)
            Spacer(Modifier.width(12.dp))
            Column(Modifier.weight(1f)) {
                Label(download.song.title, weight = FontWeight.SemiBold)
                Label(
                    when (state) {
                        DownloadState.Queued -> stringResource(R.string.download_waiting)
                        is DownloadState.Running -> state.progress
                            ?.let { stringResource(R.string.download_progress, (it * 100).toInt()) }
                            ?: stringResource(R.string.download_starting)
                        DownloadState.Done -> download.file?.let { stringResource(R.string.download_saved) }
                            ?: stringResource(R.string.download_offline_ready)
                        is DownloadState.Failed -> state.reason
                    },
                    size = 13.sp,
                    subtle = true,
                    lines = 2,
                )
                if (state is DownloadState.Running) {
                    val shown by animateFloatAsState(state.progress ?: 0f, label = "download")
                    Box(
                        Modifier.padding(top = 6.dp).fillMaxWidth().height(4.dp)
                            .clip(RoundedCornerShape(2.dp)).background(palette.muted)
                    ) {
                        Box(
                            Modifier.fillMaxWidth(shown.coerceIn(0f, 1f)).height(4.dp)
                                .clip(RoundedCornerShape(2.dp)).background(palette.accent)
                        )
                    }
                }
            }
            if (state is DownloadState.Done || state is DownloadState.Failed) {
                Spacer(Modifier.width(8.dp))
                IconPill(Icons.Rounded.Close, stringResource(R.string.remove), onRemove, size = 34.dp)
            }
        }
    }
}
