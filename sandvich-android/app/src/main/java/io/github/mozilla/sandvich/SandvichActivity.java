package io.github.mozilla.sandvich;

import android.support.v7.app.AppCompatActivity;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.widget.Button;

import io.github.mozilla.sandvich.rust.Config;
import io.github.mozilla.sandvich.rust.Error;
import io.github.mozilla.sandvich.rust.FirefoxAccount;
import io.github.mozilla.sandvich.rust.JNA;
import io.github.mozilla.sandvich.rust.RustResult;

public class SandvichActivity extends AppCompatActivity {
    static {
        System.loadLibrary("crypto");
        System.loadLibrary("ssl");
        System.loadLibrary("fxa_client");
    }

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);
        setContentView(R.layout.activity_sandvich);
        Button btn = (Button) findViewById(R.id.button);

        RustResult result = JNA.INSTANCE.fxa_get_release_config();
        Config config = new Config(result.ok);
        String clientId = "22d74070a481bc73";
        String redirectUri = "com.mozilla.sandvich:/oauth2redirect/fxa-provider";

        RustResult fxaResult = JNA.INSTANCE.fxa_new(config.rawPointer, clientId);
        FirefoxAccount fxa = new FirefoxAccount(fxaResult.ok);
        RustResult fxaFlowUrlResult = JNA.INSTANCE.fxa_begin_oauth_flow(fxa.rawPointer, redirectUri, "profile", false);
        String fxaFlowUrl = fxaFlowUrlResult.ok.getPointer(0).getString(0, "utf8");
        System.out.print("do it!");

        btn.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                RustResult result = JNA.INSTANCE.fxa_get_release_config();
                if (result.isFailure()) {
                    Log.e("Sandvich", "Failed");
                }
                Error cake = result.getError();
                Config config = new Config(result.ok);
                System.out.print("cool!");
            }
        });

    }

}
