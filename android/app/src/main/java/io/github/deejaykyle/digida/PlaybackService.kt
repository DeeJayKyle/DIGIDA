package io.github.deejaykyle.digida

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.content.pm.ServiceInfo
import android.os.Build
import android.os.IBinder
import android.support.v4.media.session.MediaSessionCompat
import android.support.v4.media.session.PlaybackStateCompat
import androidx.core.app.NotificationCompat
import androidx.core.app.ServiceCompat
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
 * so the lock screen, headphones and the quick settings player can drive the queue.
 */
class PlaybackService : Service() {
    private val scope = CoroutineScope(SupervisorJob() + Dispatchers.Main)
    private lateinit var session: MediaSessionCompat
    private val library get() = (application as DigidaApp).library

    override fun onBind(intent: Intent?): IBinder? = null

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
            })
            isActive = true
        }
        ServiceCompat.startForeground(
            this,
            NOTIFICATION,
            notification(library.status.value),
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) ServiceInfo.FOREGROUND_SERVICE_TYPE_MEDIA_PLAYBACK else 0,
        )
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
        when (intent?.action) {
            ACTION_TOGGLE -> library.toggle()
            ACTION_NEXT -> library.next()
            ACTION_PREVIOUS -> library.previous()
        }
        return START_NOT_STICKY
    }

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
            .addAction(android.R.drawable.ic_media_previous, getString(R.string.previous), action(ACTION_PREVIOUS))
            .addAction(
                if (playing) android.R.drawable.ic_media_pause else android.R.drawable.ic_media_play,
                getString(if (playing) R.string.pause else R.string.play),
                action(ACTION_TOGGLE),
            )
            .addAction(android.R.drawable.ic_media_next, getString(R.string.next), action(ACTION_NEXT))
            .setStyle(MediaStyle().setMediaSession(session.sessionToken).setShowActionsInCompactView(0, 1, 2))
            .build()
    }

    private fun action(name: String): PendingIntent = PendingIntent.getService(
        this, name.hashCode(), Intent(this, PlaybackService::class.java).setAction(name),
        PendingIntent.FLAG_IMMUTABLE,
    )

    companion object {
        private const val CHANNEL = "playback"
        private const val NOTIFICATION = 1
        private const val ACTION_TOGGLE = "toggle"
        private const val ACTION_NEXT = "next"
        private const val ACTION_PREVIOUS = "previous"

        fun start(context: Context) =
            context.startForegroundServiceCompat(Intent(context, PlaybackService::class.java))
    }
}
