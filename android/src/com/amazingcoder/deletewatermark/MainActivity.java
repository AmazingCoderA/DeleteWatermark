package com.amazingcoder.deletewatermark;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Bundle;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Toast;

import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;

/**
 * Оболочка APK: поднимает Rust-движок (веб-сервер) и показывает его UI в WebView.
 * База образцов копируется из assets в filesDir при первом запуске.
 */
public class MainActivity extends Activity {
    static {
        System.loadLibrary("delete_watermark_android");
    }

    private static final int REQ_FILE = 1001;
    private WebView web;
    private ValueCallback<Uri[]> filePathCallback;

    public native void startServer(String samplesPath, int port);

    @Override
    protected void onCreate(Bundle savedInstanceState) {
        super.onCreate(savedInstanceState);

        web = new WebView(this);
        setContentView(web);

        WebSettings settings = web.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setDomStorageEnabled(true);
        settings.setAllowFileAccess(true);
        settings.setLoadWithOverviewMode(true);
        settings.setUseWideViewPort(true);

        web.setWebViewClient(new WebViewClient());
        web.setWebChromeClient(new WebChromeClient() {
            @Override
            public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> cb,
                                             FileChooserParams params) {
                if (filePathCallback != null) {
                    filePathCallback.onReceiveValue(null);
                }
                filePathCallback = cb;
                try {
                    startActivityForResult(params.createIntent(), REQ_FILE);
                    return true;
                } catch (Exception e) {
                    filePathCallback = null;
                    return false;
                }
            }
        });

        File samples = new File(getFilesDir(), "GDZ_SAMPLES");
        copyAssets("GDZ_SAMPLES", samples);

        try {
            startServer(samples.getAbsolutePath(), 5000);
        } catch (Throwable t) {
            Toast.makeText(this, "Rust engine error: " + t.getMessage(), Toast.LENGTH_LONG).show();
        }

        // Сервер поднимается в отдельном потоке — даём ему мгновение.
        web.postDelayed(new Runnable() {
            @Override
            public void run() {
                web.loadUrl("http://127.0.0.1:5000/");
            }
        }, 700);
    }

    @Override
    protected void onActivityResult(int requestCode, int resultCode, Intent data) {
        super.onActivityResult(requestCode, resultCode, data);
        if (requestCode != REQ_FILE || filePathCallback == null) {
            return;
        }
        Uri[] results = null;
        if (resultCode == RESULT_OK && data != null) {
            String dataString = data.getDataString();
            if (dataString != null) {
                results = new Uri[]{Uri.parse(dataString)};
            } else if (data.getClipData() != null) {
                int count = data.getClipData().getItemCount();
                results = new Uri[count];
                for (int i = 0; i < count; i++) {
                    results[i] = data.getClipData().getItemAt(i).getUri();
                }
            }
        }
        filePathCallback.onReceiveValue(results);
        filePathCallback = null;
    }

    private void copyAssets(String from, File to) {
        try {
            String[] children = getAssets().list(from);
            if (children == null || children.length == 0) {
                File parent = to.getParentFile();
                if (parent != null) {
                    parent.mkdirs();
                }
                InputStream in = getAssets().open(from);
                OutputStream out = new FileOutputStream(to);
                byte[] buffer = new byte[8192];
                int n;
                while ((n = in.read(buffer)) > 0) {
                    out.write(buffer, 0, n);
                }
                out.close();
                in.close();
            } else {
                to.mkdirs();
                for (String child : children) {
                    copyAssets(from + "/" + child, new File(to, child));
                }
            }
        } catch (IOException e) {
            // Игнорируем: движок сообщит об отсутствии образцов.
        }
    }

    @Override
    public void onBackPressed() {
        if (web.canGoBack()) {
            web.goBack();
        } else {
            super.onBackPressed();
        }
    }
}
