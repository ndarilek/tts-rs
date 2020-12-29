package rs.tts

import android.app.NativeActivity
import android.speech.tts.TextToSpeech
import android.speech.tts.TextToSpeech.OnInitListener 

class MainActivity : NativeActivity(), OnInitListener {
    override fun onInit(status:Int) {
        if(status == TextToSpeech.SUCCESS) {
            println("Successfully initialized TTS!")
        } else {
            println("Failed to initialize TTS.")
        }
    }
}