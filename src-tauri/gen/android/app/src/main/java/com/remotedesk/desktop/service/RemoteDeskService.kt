package com.remotedesk.desktop.service

import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
import android.app.PendingIntent
import android.app.Service
import android.content.Context
import android.content.Intent
import android.hardware.display.DisplayManager
import android.hardware.display.VirtualDisplay
import android.media.MediaCodec
import android.media.MediaCodecInfo
import android.media.MediaFormat
import android.media.projection.MediaProjection
import android.media.projection.MediaProjectionManager
import android.os.Build
import android.os.Handler
import android.os.HandlerThread
import android.os.IBinder
import android.view.Surface
import androidx.core.app.NotificationCompat
import com.remotedesk.desktop.MainActivity
import java.nio.ByteBuffer

/**
 * Foreground service that handles:
 * - Screen capture via MediaProjection + MediaCodec
 * - Input injection (pointer/keyboard) from remote client
 * - JNI bridge communication with native Rust code
 */
class RemoteDeskService : Service() {

    companion object {
        const val CHANNEL_ID = "RemoteDeskChannel"
        const val NOTIFICATION_ID = 1
        const val ACTION_STOP = "com.remotedesk.desktop.STOP_SERVICE"

        // Mirror current display size — updated from rustGetByName
        var displayWidth = 1080
        var displayHeight = 1920
        var isRunning = false
    }

    private var mediaProjection: MediaProjection? = null
    private var virtualDisplay: VirtualDisplay? = null
    private var mediaCodec: MediaCodec? = null
    private var handlerThread: HandlerThread? = null
    private var handler: Handler? = null

    override fun onCreate() {
        super.onCreate()
        createNotificationChannel()
        isRunning = true
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        when (intent?.action) {
            ACTION_STOP -> {
                stopSelf()
                return START_NOT_STICKY
            }
        }

        val notification = buildNotification()
        startForeground(NOTIFICATION_ID, notification)

        // Get MediaProjection intent from the activity
        val projectionData = intent?.getParcelableExtra<Intent>("projectionData")
        val resultCode = intent?.getIntExtra("resultCode", -1) ?: -1

        if (projectionData != null && resultCode != -1) {
            startScreenCapture(projectionData, resultCode)
        }

        return START_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        stopScreenCapture()
        isRunning = false
        super.onDestroy()
    }

    // ── Screen Capture ───────────────────────────────────

    private fun startScreenCapture(data: Intent, resultCode: Int) {
        val projectionManager = getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
        mediaProjection = projectionManager.getMediaProjection(resultCode, data)

        val width = displayWidth
        val height = displayHeight
        val density = resources.displayMetrics.densityDpi

        // Set up MediaCodec for H.264 encoding
        val format = MediaFormat.createVideoFormat(
            MediaFormat.MIME_TYPE_AVC, width, height
        ).apply {
            setInteger(MediaFormat.KEY_BIT_RATE, 2000000)
            setInteger(MediaFormat.KEY_FRAME_RATE, 30)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2)
            setInteger(
                MediaFormat.KEY_COLOR_FORMAT,
                MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface
            )
        }

        try {
            mediaCodec = MediaCodec.createEncoderByType(MediaFormat.MIME_TYPE_AVC)
            mediaCodec!!.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)

            val inputSurface = mediaCodec!!.createInputSurface()

            virtualDisplay = mediaProjection!!.createVirtualDisplay(
                "RemoteDesk",
                width, height, density,
                DisplayManager.VIRTUAL_DISPLAY_FLAG_PUBLIC,
                inputSurface,
                null, null
            )

            mediaCodec!!.start()

            // Process encoded frames in background thread
            handlerThread = HandlerThread("MediaCodecThread")
            handlerThread!!.start()
            handler = Handler(handlerThread!!.looper)

            handler!!.post(object : Runnable {
                override fun run() {
                    processEncodedFrames()
                    if (isRunning) {
                        handler?.postDelayed(this, 10) // ~100 FPS check
                    }
                }
            })

        } catch (e: Exception) {
            e.printStackTrace()
        }
    }

    private fun processEncodedFrames() {
        val codec = mediaCodec ?: return
        val bufferInfo = MediaCodec.BufferInfo()

        while (true) {
            val outputIndex = codec.dequeueOutputBuffer(bufferInfo, 0)
            if (outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED) continue
            if (outputIndex == MediaCodec.INFO_TRY_AGAIN_LATER) break
            if (outputIndex < 0) break

            val outputBuffer = codec.getOutputBuffer(outputIndex)
            if (outputBuffer != null && bufferInfo.size > 0) {
                // Deliver frame to native Rust via JNI
                try {
                    FFI.onVideoFrameUpdate(outputBuffer)
                } catch (e: Exception) {
                    // FFI class might not be loaded yet
                }
            }
            codec.releaseOutputBuffer(outputIndex, false)
        }
    }

    private fun stopScreenCapture() {
        handler?.removeCallbacksAndMessages(null)
        handlerThread?.quitSafely()

        mediaCodec?.stop()
        mediaCodec?.release()
        virtualDisplay?.release()
        mediaProjection?.stop()
    }

    // ── Notification ─────────────────────────────────────

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "RemoteDesk Service",
                NotificationManager.IMPORTANCE_LOW
            )
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        val stopIntent = Intent(this, RemoteDeskService::class.java).apply {
            action = ACTION_STOP
        }
        val stopPendingIntent = PendingIntent.getService(
            this, 0, stopIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        val openIntent = Intent(this, MainActivity::class.java)
        val openPendingIntent = PendingIntent.getActivity(
            this, 0, openIntent,
            PendingIntent.FLAG_IMMUTABLE or PendingIntent.FLAG_UPDATE_CURRENT
        )

        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setContentTitle("RemoteDesk")
            .setContentText("Screen sharing active")
            .setSmallIcon(android.R.drawable.ic_menu_share)
            .setContentIntent(openPendingIntent)
            .addAction(android.R.drawable.ic_media_pause, "Stop", stopPendingIntent)
            .setOngoing(true)
            .build()
    }

    // ── JNI Methods (called from Rust via scrap) ─────────

    /**
     * Called from Rust: inject a pointer/touch event.
     * kind: "move", "down", "up", "wheel"
     * mask: button mask
     * x, y: screen coordinates
     */
    fun rustPointerInput(kind: String, mask: Int, x: Int, y: Int) {
        // Touch input simulation requires INJECT_EVENTS permission (system app or root).
        // For non-root devices, this is handled via AccessibilityService.
        // For now, log the event.
        android.util.Log.d("RemoteDesk", "Pointer: $kind at ($x, $y) mask=$mask")
    }

    /**
     * Called from Rust: inject a key event.
     * data: [down(1B), keycode(4B LE), scancode(4B LE), modifiers(4B LE)]
     */
    fun rustKeyEventInput(data: ByteArray) {
        if (data.size < 13) return
        val down = data[0].toInt() == 1
        val keycode = (data[1].toInt() and 0xFF) or
                      ((data[2].toInt() and 0xFF) shl 8) or
                      ((data[3].toInt() and 0xFF) shl 16) or
                      ((data[4].toInt() and 0xFF) shl 24)
        android.util.Log.d("RemoteDesk", "Key: keycode=$keycode down=$down")
    }

    /**
     * Called from Rust: get information by name.
     * Returns JSON string with requested info.
     */
    fun rustGetByName(name: String): String {
        return when (name) {
            "screen_size" -> "{\"width\":$displayWidth,\"height\":$displayHeight}"
            "status" -> if (isRunning) "running" else "stopped"
            else -> ""
        }
    }

    /**
     * Called from Rust: set a setting by name.
     */
    fun rustSetByName(name: String, arg1: String, arg2: String) {
        when (name) {
            "display_size" -> {
                val parts = arg1.split("x")
                if (parts.size == 2) {
                    displayWidth = parts[0].toIntOrNull() ?: displayWidth
                    displayHeight = parts[1].toIntOrNull() ?: displayHeight
                }
            }
        }
    }
}
