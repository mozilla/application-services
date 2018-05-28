package io.github.mozilla.sandvich;

import android.support.v7.app.AppCompatActivity;
import android.os.Bundle;
import android.util.Log;
import android.view.View;
import android.widget.Button;

import io.github.mozilla.sandvich.rust.Config;
import io.github.mozilla.sandvich.rust.Error;
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
