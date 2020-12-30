package rs.tts

import android.app.NativeActivity

class MainActivity : NativeActivity() {
    companion object {
        init {
            System.loadLibrary("hello_world")
        }
    }
}