package rs.tts;

import android.speech.tts.TextToSpeech;

@androidx.annotation.Keep
public class Bridge implements TextToSpeech.OnInitListener {
    public int backendId;

    public Bridge(int backendId) {
        this.backendId = backendId;
    }

        public native void onInit(int status);

}