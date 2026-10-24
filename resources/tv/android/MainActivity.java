package local.streamvault;

import android.app.Activity;
import android.graphics.Color;
import android.net.Uri;
import android.os.Build;
import android.os.Bundle;
import android.util.Base64;
import android.webkit.JavascriptInterface;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebSettings;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.window.OnBackInvokedDispatcher;
import java.io.ByteArrayInputStream;
import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import java.util.UUID;
import java.util.concurrent.ConcurrentHashMap;

public final class MainActivity extends Activity {
    private static final String HOST = "appassets.androidplatform.net";
    private static final String INDEX = "index.html";
    private static final String BODY = "x-streamvault-body";
    private static final int TIMEOUT_MS = 30_000;
    private static final int MAX_REDIRECTS = 20;
    private static final Map<String, String> TYPES = Map.of(
            "html", "text/html",
            "js", "text/javascript",
            "css", "text/css",
            "wasm", "application/wasm",
            "json", "application/json",
            "png", "image/png",
            "svg", "image/svg+xml");
    private static final String BRIDGE = String.format(Locale.ROOT, """
            <script>
              (() => {
                const send = window.fetch.bind(window);
                window.fetch = async (input, init) => {
                  const request = new Request(input, init);
                  if (["GET", "HEAD"].includes(request.method) || new URL(request.url).origin === location.origin) {
                    return send(request);
                  }
                  let binary = "";
                  for (const byte of new Uint8Array(await request.clone().arrayBuffer())) {
                    binary += String.fromCharCode(byte);
                  }
                  const headers = new Headers(request.headers);
                  headers.set("%s", StreamVault.keep(btoa(binary)));
                  return send(new Request(request, { headers }));
                };
              })();
            </script>
            """, BODY);

    private final Map<String, byte[]> bodies = new ConcurrentHashMap<>();
    private WebView web;

    @Override
    protected void onCreate(Bundle state) {
        super.onCreate(state);
        web = new WebView(this);
        web.setBackgroundColor(Color.TRANSPARENT);
        WebSettings settings = web.getSettings();
        settings.setJavaScriptEnabled(true);
        settings.setMediaPlaybackRequiresUserGesture(false);
        settings.setUseWideViewPort(true);
        settings.setLoadWithOverviewMode(true);
        web.addJavascriptInterface(this, "StreamVault");
        web.setWebChromeClient(new WebChromeClient() {
            @Override
            public void onCloseWindow(WebView window) {
                finish();
            }
        });
        web.setWebViewClient(new WebViewClient() {
            @Override
            public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                return intercept(request);
            }
        });
        setContentView(web);
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.TIRAMISU) {
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                    OnBackInvokedDispatcher.PRIORITY_DEFAULT, this::back);
        }
        web.loadUrl("https://" + HOST + "/" + INDEX);
    }

    @Override
    public void onBackPressed() {
        back();
    }

    @JavascriptInterface
    public String keep(String body) {
        String id = UUID.randomUUID().toString();
        bodies.put(id, Base64.decode(body, Base64.DEFAULT));
        return id;
    }

    private void back() {
        web.evaluateJavascript(
                "dispatchEvent(new KeyboardEvent('keydown', { key: 'GoBack', cancelable: true }))",
                handled -> {
                    if (!handled.equals("false")) {
                        finish();
                    }
                });
    }

    private WebResourceResponse intercept(WebResourceRequest request) {
        Uri url = request.getUrl();
        if (HOST.equals(url.getHost())) {
            return asset(url.getPath());
        }
        Map<String, String> headers = new TreeMap<>(String.CASE_INSENSITIVE_ORDER);
        headers.putAll(request.getRequestHeaders());
        String origin = headers.remove("Origin");
        if (origin == null) {
            return null;
        }
        String method = request.getMethod();
        if (method.equals("OPTIONS") && headers.containsKey("Access-Control-Request-Method")) {
            return preflight(origin, headers);
        }
        String id = headers.remove(BODY);
        byte[] body = id == null ? null : bodies.remove(id);
        if (body == null && !method.equals("GET") && !method.equals("HEAD")) {
            return null;
        }
        headers.remove("Referer");
        try {
            return forward(url.toString(), method, headers, body, origin);
        } catch (IOException e) {
            return new WebResourceResponse(null, null, null);
        }
    }

    private WebResourceResponse asset(String path) {
        String name = path == null || path.equals("/") ? INDEX : path.substring(1);
        String extension = name.substring(name.lastIndexOf('.') + 1);
        try {
            InputStream data = name.equals(INDEX) ? index() : getAssets().open("www/" + name);
            return new WebResourceResponse(TYPES.get(extension), null, data);
        } catch (IOException e) {
            return new WebResourceResponse("text/plain", null, 404, "Not Found", Map.of(), empty());
        }
    }

    private InputStream index() throws IOException {
        try (InputStream in = getAssets().open("www/" + INDEX)) {
            ByteArrayOutputStream html = new ByteArrayOutputStream();
            byte[] buffer = new byte[8192];
            for (int read; (read = in.read(buffer)) > 0; ) {
                html.write(buffer, 0, read);
            }
            String page = html.toString(StandardCharsets.UTF_8.name()).replace("<head>", "<head>" + BRIDGE);
            return new ByteArrayInputStream(page.getBytes(StandardCharsets.UTF_8));
        }
    }

    private static WebResourceResponse preflight(String origin, Map<String, String> headers) {
        Map<String, String> allow = allow(origin);
        allow.put("Access-Control-Allow-Methods", headers.get("Access-Control-Request-Method"));
        String requested = headers.get("Access-Control-Request-Headers");
        if (requested != null) {
            allow.put("Access-Control-Allow-Headers", requested);
        }
        allow.put("Access-Control-Max-Age", "600");
        return new WebResourceResponse("text/plain", null, 204, "No Content", allow, empty());
    }

    private static WebResourceResponse forward(
            String url, String method, Map<String, String> headers, byte[] body, String origin) throws IOException {
        HttpURLConnection connection = open(url, method, headers, body);
        int status = connection.getResponseCode();
        if (status < 200 || status > 599 || (status >= 300 && status < 400)) {
            throw new IOException("unusable status " + status);
        }
        Map<String, String> response = allow(origin);
        StringBuilder exposed = new StringBuilder();
        for (Map.Entry<String, List<String>> header : connection.getHeaderFields().entrySet()) {
            if (header.getKey() != null && !header.getKey().equalsIgnoreCase("Content-Type")) {
                response.put(header.getKey(), String.join(", ", header.getValue()));
                exposed.append(exposed.length() == 0 ? "" : ", ").append(header.getKey());
            }
        }
        response.put("Access-Control-Expose-Headers", exposed.toString());
        String type = connection.getContentType();
        String reason = connection.getResponseMessage();
        InputStream data = status >= 400 ? connection.getErrorStream() : connection.getInputStream();
        return new WebResourceResponse(
                type == null ? null : type.split(";")[0].trim(),
                charset(type),
                status,
                reason != null && reason.matches("[\\x20-\\x7E]+") ? reason : String.valueOf(status),
                response,
                data == null ? empty() : data);
    }

    private static HttpURLConnection open(String url, String method, Map<String, String> headers, byte[] body)
            throws IOException {
        for (int redirects = 0; redirects <= MAX_REDIRECTS; redirects++) {
            HttpURLConnection connection = (HttpURLConnection) new URL(url).openConnection();
            connection.setInstanceFollowRedirects(false);
            connection.setConnectTimeout(TIMEOUT_MS);
            connection.setReadTimeout(TIMEOUT_MS);
            connection.setRequestMethod(method);
            for (Map.Entry<String, String> header : headers.entrySet()) {
                connection.setRequestProperty(header.getKey(), header.getValue());
            }
            if (body != null) {
                connection.setDoOutput(true);
                try (OutputStream out = connection.getOutputStream()) {
                    out.write(body);
                }
            }
            int status = connection.getResponseCode();
            String location = connection.getHeaderField("Location");
            if (status < 300 || status >= 400 || location == null) {
                return connection;
            }
            connection.disconnect();
            url = new URL(new URL(url), location).toString();
            if (status == 303 || ((status == 301 || status == 302) && method.equals("POST"))) {
                method = "GET";
                body = null;
            }
        }
        throw new IOException("too many redirects");
    }

    private static Map<String, String> allow(String origin) {
        Map<String, String> headers = new HashMap<>();
        headers.put("Access-Control-Allow-Origin", origin);
        headers.put("Access-Control-Allow-Credentials", "true");
        return headers;
    }

    private static String charset(String type) {
        if (type == null) {
            return null;
        }
        for (String parameter : type.split(";")) {
            String[] pair = parameter.trim().split("=", 2);
            if (pair.length == 2 && pair[0].equalsIgnoreCase("charset")) {
                return pair[1].replace("\"", "");
            }
        }
        return null;
    }

    private static InputStream empty() {
        return new ByteArrayInputStream(new byte[0]);
    }
}
