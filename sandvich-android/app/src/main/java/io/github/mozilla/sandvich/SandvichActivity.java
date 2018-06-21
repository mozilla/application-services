package io.github.mozilla.sandvich;

import android.content.Intent;
import android.net.Uri;
import android.support.customtabs.CustomTabsIntent;
import android.support.v7.app.AppCompatActivity;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.widget.Button;

import io.github.mozilla.sandvich.rust.Config;
import io.github.mozilla.sandvich.rust.FirefoxAccount;

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

        final CustomTabsIntent customTabsIntent = new CustomTabsIntent.Builder()
            .addDefaultShareMenuItem()
            .setShowTitle(true)
            .build();

        Config config = Config.custom("https://sandvich-ios.dev.lcip.org");

        String clientId = "22d74070a481bc73";
        String redirectUri = "https://mozilla-lockbox.github.io/fxa/ios-redirect.html";

        FirefoxAccount fxa = new FirefoxAccount(config, clientId);
        String[] scopes = new String[] {"profile"};
        final String flowUrl = fxa.beginOAuthFlow(redirectUri, scopes, false);

        btn.setOnClickListener(new View.OnClickListener() {
            @Override
            public void onClick(View v) {
                Log.i("sandvich", "Starting FxA login");
                customTabsIntent.launchUrl(SandvichActivity.this, Uri.parse(flowUrl));
            }
        });
    }

    @Override
    protected void onStart() {
        super.onStart();

    }
}
