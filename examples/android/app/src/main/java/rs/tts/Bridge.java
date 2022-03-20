package rs.tts;

import android.speech.tts.TextToSpeech;
import android.speech.tts.UtteranceProgressListener;

@androidx.annotation.Keep
public class Bridge extends UtteranceProgressListener implements TextToSpeech.OnInitListener {
    public int backendId;

    public Bridge(int backendId) {
        this.backendId = backendId;
    }

    public native void onInit(int status);

    public native void onStart(String utteranceId);

    public native void onStop(String utteranceId, Boolean interrupted);

    public native void onDone(String utteranceId);

    public native void onError(String utteranceId) ;

}