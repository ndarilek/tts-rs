package rs.tts;

import android.speech.tts.TextToSpeech;
import android.speech.tts.UtteranceProgressListener;

/**
 * Relays Android TTS callbacks into the Rust backend.
 *
 * <p>Compiled to <i>bridge.dex</i> by the crate's build script and loaded at runtime, so consuming
 * apps need no Java of their own. The methods below are bound with {@code RegisterNatives} and must
 * stay in step with {@code BRIDGE_METHODS} in <i>src/backends/android.rs</i>. {@code backendId}
 * identifies which Rust-side backend a callback belongs to.
 */
public class Bridge extends UtteranceProgressListener implements TextToSpeech.OnInitListener {
    public int backendId;

    public Bridge(int backendId) {
        this.backendId = backendId;
    }

    public native void onInit(int status);

    @Override
    public native void onStart(String utteranceId);

    @Override
    public native void onStop(String utteranceId, boolean interrupted);

    @Override
    public native void onDone(String utteranceId);

    @Override
    public native void onError(String utteranceId);
}
