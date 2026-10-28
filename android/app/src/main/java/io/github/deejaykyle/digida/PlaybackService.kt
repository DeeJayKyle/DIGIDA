package io.github.deejaykyle.digida

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.Bundle
import android.support.v4.media.MediaBrowserCompat
import android.support.v4.media.MediaDescriptionCompat
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
import androidx.media.MediaBrowserServiceCompat
import androidx.media.app.NotificationCompat.MediaStyle
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.cancel
import kotlinx.coroutines.flow.distinctUntilChanged
import kotlinx.coroutines.flow.map
import kotlinx.coroutines.launch
import uniffi.digida_mobile.PlayState
import uniffi.digida_mobile.Status

/**
 * Keeps the process alive while music plays, with the media notification and a media session
 * so the lock screen, headphones and the quick settings player can drive the queue. It is also
 * the media browser Android Auto binds to, listing the queue and the library on the car's screen.
 */
class PlaybackService : MediaBrowserServiceCompat() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private lateinit var session: MediaSessionCompat
    private val library get() = (application as DigidaApp).library
    /** The cover the widget last drew, and the song it belongs to, so a tick never reloads it. */
    private var cover: Pair<String?, android.graphics.Bitmap?> = null to null

    override fun onCreate() {
        super.onCreate()
        val manager = getSystemService(NotificationManager::class.java)
        manager.createNotificationChannel(
            NotificationChannel(CHANNEL, getString(R.string.playing_channel), NotificationManager.IMPORTANCE_LOW)
        )
        session = MediaSessionCompat(this, "digida").apply {
            setCallback(object : MediaSessionCompat.Callback() {
                override fun onPlay() = library.toggle()
                override fun onPause() = library.toggle()
                override fun onSkipToNext() = library.next()
                override fun onSkipToPrevious() = library.previous()
                override fun onSeekTo(pos: Long) = library.seek(pos)
                override fun onPlayFromMediaId(mediaId: String?, extras: Bundle?) = playFrom(mediaId)
            })
            isActive = true
        }
        sessionToken = session.sessionToken
        scope.launch {
            library.status
                .map { Triple(it.state, it.song?.id, it.positionMs / 1000u) }
                .distinctUntilChanged()
                .collect {
                    val status = library.status.value
                    publish(status)
                    if (status.state == PlayState.IDLE) {
                        ServiceCompat.stopForeground(this@PlaybackService, ServiceCompat.STOP_FOREGROUND_DETACH)
                        stopSelf()
                    }
                }
        }
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        // started in the foreground by the app, a notification button or the widget; a car that
        // only binds to browse leaves the service in the background until something plays
        ServiceCompat.startForeground(
            this,
            NOTIFICATION,
            notification(library.status.value),
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else 0,
        )
        when (intent?.action) {
            ACTION_TOGGLE -> library.toggle()
            ACTION_NEXT -> library.next()
            ACTION_PREVIOUS -> library.previous()
        }
        return START_NOT_STICKY
    }

    override fun onGetRoot(clientPackageName: String, clientUid: Int, rootHints: Bundle?): MediaBrowserServiceCompat.BrowserRoot =
        MediaBrowserServiceCompat.BrowserRoot(ROOT, null)

    override fun onLoadChildren(parentId: String, result: MediaBrowserServiceCompat.Result<MutableList<MediaBrowserCompat.MediaItem>>) {
        when (parentId) {
            ROOT -> result.sendResult(
                mutableListOf(
                    folder(QUEUE, getString(R.string.queue)),
                    folder(SONGS, getString(R.string.songs)),
                )
            )
            QUEUE -> {
                result.detach()
                scope.launch {
                    val songs = runCatching { library.queue() }.getOrDefault(emptyList())
                    result.sendResult(songs.mapIndexed { at, song -> playable("$QUEUE/$at", song) }.toMutableList())
                }
            }
            SONGS -> result.sendResult(
                library.shelf.value.songs.take(BROWSE_LIMIT)
                    .mapIndexed { at, song -> playable("$SONGS/$at", song) }.toMutableList()
            )
            else -> result.sendResult(mutableListOf())
        }
    }

    /** Plays what a car picked: a place in the queue, or the library's songs from one of them. */
    private fun playFrom(mediaId: String?) {
        val (list, at) = mediaId?.split('/')?.takeIf { it.size == 2 } ?: return
        val index = at.toIntOrNull() ?: return
        when (list) {
            QUEUE -> library.jump(index)
            SONGS -> library.play(library.shelf.value.songs.take(BROWSE_LIMIT), index)
        }
    }

    private fun folder(id: String, title: String) = MediaBrowserCompat.MediaItem(
        MediaDescriptionCompat.Builder().setMediaId(id).setTitle(title).build(),
        MediaBrowserCompat.MediaItem.FLAG_BROWSABLE,
    )

    private fun playable(id: String, song: uniffi.digida_mobile.Song) = MediaBrowserCompat.MediaItem(
        MediaDescriptionCompat.Builder()
            .setMediaId(id)
            .setTitle(song.title)
            .setSubtitle(song.artist)
            .setIconUri(song.cover?.let { android.net.Uri.parse(it) })
            .build(),
        MediaBrowserCompat.MediaItem.FLAG_PLAYABLE,
    )

    override fun onDestroy() {
        scope.cancel()
        session.release()
        super.onDestroy()
    }

    /** Puts the status on the media session and redraws the notification. */
    private fun publish(status: Status) {
        val playing = status.state == PlayState.PLAYING
        session.setPlaybackState(
            PlaybackStateCompat.Builder()
                .setActions(
                    PlaybackStateCompat.ACTION_PLAY_PAUSE or PlaybackStateCompat.ACTION_PLAY or
                        PlaybackStateCompat.ACTION_PAUSE or PlaybackStateCompat.ACTION_SKIP_TO_NEXT or
                        PlaybackStateCompat.ACTION_SKIP_TO_PREVIOUS or PlaybackStateCompat.ACTION_SEEK_TO
                )
                .setState(
                    if (playing) PlaybackStateCompat.STATE_PLAYING else PlaybackStateCompat.STATE_PAUSED,
                    status.positionMs.toLong(),
                    if (playing) 1f else 0f,
                )
                .build()
        )
        session.setMetadata(
            android.support.v4.media.MediaMetadataCompat.Builder()
                .putString(android.support.v4.media.MediaMetadataCompat.METADATA_KEY_TITLE, status.song?.title)
                .putString(android.support.v4.media.MediaMetadataCompat.METADATA_KEY_ARTIST, status.song?.artist)
                .putString(android.support.v4.media.MediaMetadataCompat.METADATA_KEY_ALBUM, status.song?.album)
                .putLong(android.support.v4.media.MediaMetadataCompat.METADATA_KEY_DURATION, status.durationMs.toLong())
                .build()
        )
        getSystemService(NotificationManager::class.java).notify(NOTIFICATION, notification(status))
        val id = status.song?.id
        if (cover.first == id) {
            NowPlayingWidget.update(this, status, cover.second)
        } else {
            NowPlayingWidget.update(this, status, null)
            scope.launch {
                val bitmap = NowPlayingWidget.cover(this@PlaybackService, status.song?.cover)
                cover = id to bitmap
                NowPlayingWidget.update(this@PlaybackService, library.status.value, bitmap)
            }
        }
    }

    private fun notification(status: Status): Notification {
        val playing = status.state == PlayState.PLAYING
        val open = PendingIntent.getActivity(
            this, 0, Intent(this, MainActivity::class.java), PendingIntent.FLAG_IMMUTABLE,
        )
        return NotificationCompat.Builder(this, CHANNEL)
            .setSmallIcon(R.drawable.ic_notification)
            .setContentTitle(status.song?.title ?: getString(R.string.nothing_playing))
            .setContentText(status.song?.artist)
            .setContentIntent(open)
            .setOngoing(playing)
            .setSilent(true)
            .setVisibility(NotificationCompat.VISIBILITY_PUBLIC)
            .addAction(android.R.drawable.ic_media_previous, getString(R.string.previous), action(this, ACTION_PREVIOUS))
            .addAction(
                if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
                getString(if (playing) R.string.pause else R.string.play),
                action(this, ACTION_TOGGLE),
            )
            .addAction(android.R.drawable.ic_media_next, getString(R.string.next), action(this, ACTION_NEXT))
            .setStyle(MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0, 1, 2))
            .build()
    }

    companion object {
        private const val CHANNEL = "playback"
        private const val NOTIFICATION = 1
        const val ACTION_TOGGLE = "toggle"
        const val ACTION_NEXT = "next"
        const val ACTION_PREVIOUS = "previous"
        /** The car's browse tree: a root holding the queue and the library's songs. */
        private const val ROOT = "root"
        private const val QUEUE = "queue"
        private const val SONGS = "songs"
        /** How many songs a car lists, which keeps the list quick to scroll while driving. */
        private const val BROWSE_LIMIT = 200

        /**
         * What a notification or widget button sends: the service, started in the foreground so
         * a press from the home screen reaches it even while the app is in the background.
         */
        fun action(context: Context, name: String): PendingIntent = PendingIntent.getForegroundService(
            context, name.hashCode(), Intent(context, PlaybackService::class.java).setAction(name),
            PendingIntent.FLAG_IMMUTABLE,
        )

        fun start(context: Context) =
            context.startForegroundServiceCompat(Intent(context, PlaybackService::class.java))
    }
}
