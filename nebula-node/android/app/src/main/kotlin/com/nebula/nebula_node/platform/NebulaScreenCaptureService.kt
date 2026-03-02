package com.nebula.nebula_node.platform

import android.app.Activity
import android.app.Notification
import android.app.NotificationChannel
import android.app.NotificationManager
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
import android.os.IBinder
import android.util.Log
import android.view.Surface
import androidx.core.app.NotificationCompat
import org.json.JSONObject
import java.nio.ByteBuffer
import java.util.concurrent.atomic.AtomicBoolean
import java.util.concurrent.atomic.AtomicReference

/**
 * Foreground service for screen capture via MediaProjection.
 *
 * Creates a VirtualDisplay piped into a MediaCodec H.264 encoder.
 * Encoded frames are stored in a circular buffer for retrieval by
 * [NebulaPlatformBridge.getScreenFrame].
 *
 * The MediaProjection consent must be triggered from an Activity.
 * Call [requestScreenCapture] from an Activity context, store the result
 * in [pendingResultCode] / [pendingResultData], then start this service.
 */
class NebulaScreenCaptureService : Service() {

    companion object {
        private const val TAG = "NebulaScreenCapture"
        private const val NOTIFICATION_ID = 3002
        private const val CHANNEL_ID = "nebula_screen_capture"

        const val EXTRA_WIDTH = "width"
        const val EXTRA_HEIGHT = "height"
        const val EXTRA_FPS = "fps"
        const val EXTRA_BITRATE = "bitrate"

        @Volatile
        var instance: NebulaScreenCaptureService? = null
            private set

        // Store MediaProjection consent result from an Activity
        @Volatile
        var pendingResultCode: Int = Activity.RESULT_CANCELED

        @Volatile
        var pendingResultData: Intent? = null

        /**
         * The latest encoded H.264 frame (NAL unit).
         * Updated atomically by the encoder thread.
         */
        val latestFrame = AtomicReference<ByteArray?>(null)

        /**
         * SPS (Sequence Parameter Set) extracted from the encoder.
         */
        @Volatile
        var spsData: ByteArray? = null
            private set

        /**
         * PPS (Picture Parameter Set) extracted from the encoder.
         */
        @Volatile
        var ppsData: ByteArray? = null
            private set

        /**
         * Current capture configuration.
         */
        @Volatile
        var captureWidth: Int = 0
            private set

        @Volatile
        var captureHeight: Int = 0
            private set

        @Volatile
        var captureFps: Int = 0
            private set

        @Volatile
        var captureBitrate: Int = 0
            private set

        val isActive = AtomicBoolean(false)

        /**
         * Request MediaProjection consent from an Activity.
         * The Activity must call startActivityForResult with the returned Intent
         * and store the result via [storeProjectionResult].
         */
        @JvmStatic
        fun getProjectionIntent(context: Context): Intent {
            val projectionManager = context.getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
            return projectionManager.createScreenCaptureIntent()
        }

        /**
         * Store the result from the MediaProjection consent activity.
         */
        @JvmStatic
        fun storeProjectionResult(resultCode: Int, data: Intent?) {
            pendingResultCode = resultCode
            pendingResultData = data
        }

        /**
         * Get the current capture config as a JSON string.
         */
        @JvmStatic
        fun getCaptureConfig(): String {
            return JSONObject().apply {
                put("width", captureWidth)
                put("height", captureHeight)
                put("fps", captureFps)
                put("bitrate", captureBitrate)
                put("active", isActive.get())
                put("hasSps", spsData != null)
                put("hasPps", ppsData != null)
            }.toString()
        }
    }

    private var mediaProjection: MediaProjection? = null
    private var virtualDisplay: VirtualDisplay? = null
    private var mediaCodec: MediaCodec? = null
    private var inputSurface: Surface? = null
    private var encoderThread: Thread? = null

    override fun onCreate() {
        super.onCreate()
        instance = this
        createNotificationChannel()
    }

    override fun onStartCommand(intent: Intent?, flags: Int, startId: Int): Int {
        val width = intent?.getIntExtra(EXTRA_WIDTH, 720) ?: 720
        val height = intent?.getIntExtra(EXTRA_HEIGHT, 1280) ?: 1280
        val fps = intent?.getIntExtra(EXTRA_FPS, 15) ?: 15
        val bitrate = intent?.getIntExtra(EXTRA_BITRATE, 1_000_000) ?: 1_000_000

        startForeground(NOTIFICATION_ID, buildNotification())

        if (!startCapture(width, height, fps, bitrate)) {
            Log.e(TAG, "Failed to start screen capture")
            stopSelf()
        }

        return START_NOT_STICKY
    }

    override fun onBind(intent: Intent?): IBinder? = null

    override fun onDestroy() {
        stopCapture()
        instance = null
        super.onDestroy()
    }

    private fun startCapture(width: Int, height: Int, fps: Int, bitrate: Int): Boolean {
        if (pendingResultCode != Activity.RESULT_OK || pendingResultData == null) {
            Log.e(TAG, "No valid MediaProjection consent result")
            return false
        }

        captureWidth = width
        captureHeight = height
        captureFps = fps
        captureBitrate = bitrate

        val projectionManager = getSystemService(Context.MEDIA_PROJECTION_SERVICE) as MediaProjectionManager
        mediaProjection = projectionManager.getMediaProjection(pendingResultCode, pendingResultData!!)

        if (mediaProjection == null) {
            Log.e(TAG, "Failed to obtain MediaProjection")
            return false
        }

        // Set up H.264 encoder
        val format = MediaFormat.createVideoFormat(MediaFormat.MIMETYPE_VIDEO_AVC, width, height).apply {
            setInteger(MediaFormat.KEY_COLOR_FORMAT, MediaCodecInfo.CodecCapabilities.COLOR_FormatSurface)
            setInteger(MediaFormat.KEY_BIT_RATE, bitrate)
            setInteger(MediaFormat.KEY_FRAME_RATE, fps)
            setInteger(MediaFormat.KEY_I_FRAME_INTERVAL, 2)
        }

        mediaCodec = MediaCodec.createEncoderByType(MediaFormat.MIMETYPE_VIDEO_AVC)
        mediaCodec!!.configure(format, null, null, MediaCodec.CONFIGURE_FLAG_ENCODE)
        inputSurface = mediaCodec!!.createInputSurface()
        mediaCodec!!.start()

        // Create VirtualDisplay that renders into the encoder's input surface
        val densityDpi = resources.displayMetrics.densityDpi
        virtualDisplay = mediaProjection!!.createVirtualDisplay(
            "NebulaScreenCapture",
            width, height, densityDpi,
            DisplayManager.VIRTUAL_DISPLAY_FLAG_AUTO_MIRROR,
            inputSurface, null, null
        )

        isActive.set(true)

        // Start encoder output thread
        encoderThread = Thread({
            drainEncoder()
        }, "NebulaEncoderThread").apply { start() }

        Log.i(TAG, "Screen capture started: ${width}x${height} @${fps}fps, ${bitrate}bps")
        return true
    }

    private fun stopCapture() {
        isActive.set(false)

        encoderThread?.interrupt()
        encoderThread = null

        virtualDisplay?.release()
        virtualDisplay = null

        mediaCodec?.let { codec ->
            try {
                codec.stop()
                codec.release()
            } catch (e: Exception) {
                Log.w(TAG, "Error stopping codec: ${e.message}")
            }
        }
        mediaCodec = null
        inputSurface = null

        mediaProjection?.stop()
        mediaProjection = null

        latestFrame.set(null)
        spsData = null
        ppsData = null

        Log.i(TAG, "Screen capture stopped")
    }

    /**
     * Drain encoded frames from the MediaCodec output buffer.
     * Runs on a dedicated thread. Extracts SPS/PPS from codec-specific data
     * and stores each encoded frame for retrieval.
     */
    private fun drainEncoder() {
        val bufferInfo = MediaCodec.BufferInfo()
        val codec = mediaCodec ?: return

        while (isActive.get() && !Thread.currentThread().isInterrupted) {
            val outputIndex = codec.dequeueOutputBuffer(bufferInfo, 10_000) // 10ms timeout

            when {
                outputIndex == MediaCodec.INFO_OUTPUT_FORMAT_CHANGED -> {
                    // Extract SPS and PPS from the new output format
                    val newFormat = codec.outputFormat
                    val spsBuf = newFormat.getByteBuffer("csd-0")
                    val ppsBuf = newFormat.getByteBuffer("csd-1")
                    if (spsBuf != null) {
                        spsData = ByteArray(spsBuf.remaining()).also { spsBuf.get(it) }
                    }
                    if (ppsBuf != null) {
                        ppsData = ByteArray(ppsBuf.remaining()).also { ppsBuf.get(it) }
                    }
                    Log.d(TAG, "Encoder format changed: SPS=${spsData?.size}B, PPS=${ppsData?.size}B")
                }
                outputIndex >= 0 -> {
                    val outputBuffer = codec.getOutputBuffer(outputIndex)
                    if (outputBuffer != null && bufferInfo.size > 0) {
                        outputBuffer.position(bufferInfo.offset)
                        outputBuffer.limit(bufferInfo.offset + bufferInfo.size)
                        val frameData = ByteArray(bufferInfo.size)
                        outputBuffer.get(frameData)
                        latestFrame.set(frameData)
                    }
                    codec.releaseOutputBuffer(outputIndex, false)

                    if ((bufferInfo.flags and MediaCodec.BUFFER_FLAG_END_OF_STREAM) != 0) {
                        Log.d(TAG, "Encoder end of stream")
                        break
                    }
                }
                // INFO_TRY_AGAIN_LATER (-1) -- just loop
            }
        }
    }

    private fun createNotificationChannel() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
            val channel = NotificationChannel(
                CHANNEL_ID,
                "Screen Capture",
                NotificationManager.IMPORTANCE_LOW
            ).apply {
                description = "Screen capture is active"
            }
            val manager = getSystemService(NotificationManager::class.java)
            manager.createNotificationChannel(channel)
        }
    }

    private fun buildNotification(): Notification {
        return NotificationCompat.Builder(this, CHANNEL_ID)
            .setSmallIcon(android.R.drawable.ic_dialog_info)
            .setContentTitle("NEBULA Screen Capture")
            .setContentText("Screen is being captured")
            .setPriority(NotificationCompat.PRIORITY_LOW)
            .setOngoing(true)
            .build()
    }
}
