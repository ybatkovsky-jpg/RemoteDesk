package ffi

import java.nio.ByteBuffer

/**
 * JNI bridge class for native Rust code (scrap).
 * Receives video/audio frames and clipboard data from the host side.
 */
object FFI {
    /**
     * Called from Rust via JNI when a new video frame is available.
     */
    @JvmStatic
    external fun onVideoFrameUpdate(buffer: ByteBuffer)

    /**
     * Called from Rust via JNI when a new audio frame is available.
     */
    @JvmStatic
    external fun onAudioFrameUpdate(buffer: ByteBuffer)

    /**
     * Called from Rust via JNI on clipboard change.
     */
    @JvmStatic
    external fun onClipboardUpdate(content: String)

    /**
     * Called from Rust via JNI on app start (initialization).
     */
    @JvmStatic
    external fun onAppStart(context: android.content.Context)
}
