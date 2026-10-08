package ai.storyteller.photocraft;

import android.app.NativeActivity;
import android.content.Intent;
import android.database.Cursor;
import android.net.Uri;
import android.os.Bundle;
import android.provider.OpenableColumns;
import android.view.View;
import android.view.WindowInsets;
import android.view.WindowInsetsController;
import org.json.JSONObject;
import java.io.File;
import java.io.FileOutputStream;
import java.io.InputStream;
import java.io.OutputStream;
import java.util.concurrent.ConcurrentLinkedQueue;

/** System integration only. The editor and every UI widget run in native Rust/egui. */
public final class PhotoCraftActivity extends NativeActivity {
    private static final int OPEN = 1001, SAVE = 1002;
    private static final long MAX_IMPORT_BYTES = 256L * 1024 * 1024;
    private final ConcurrentLinkedQueue<String> results = new ConcurrentLinkedQueue<>();
    private volatile boolean pending;
    private volatile int insetLeft, insetTop, insetRight, insetBottom;
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        android.view.View decor = getWindow().getDecorView();
        decor.setOnApplyWindowInsetsListener((view, insets) -> {
            if (android.os.Build.VERSION.SDK_INT >= 30) {
                android.graphics.Insets safe = insets.getInsets(android.view.WindowInsets.Type.systemBars()
                    | android.view.WindowInsets.Type.displayCutout() | android.view.WindowInsets.Type.ime());
                insetLeft = safe.left; insetTop = safe.top; insetRight = safe.right; insetBottom = safe.bottom;
            } else {
                insetLeft = insets.getSystemWindowInsetLeft(); insetTop = insets.getSystemWindowInsetTop();
                insetRight = insets.getSystemWindowInsetRight(); insetBottom = insets.getSystemWindowInsetBottom();
                if (android.os.Build.VERSION.SDK_INT >= 28 && insets.getDisplayCutout() != null) {
                    android.view.DisplayCutout cutout = insets.getDisplayCutout();
                    insetLeft = Math.max(insetLeft, cutout.getSafeInsetLeft());
                    insetTop = Math.max(insetTop, cutout.getSafeInsetTop());
                    insetRight = Math.max(insetRight, cutout.getSafeInsetRight());
                    insetBottom = Math.max(insetBottom, cutout.getSafeInsetBottom());
                }
            }
            return insets;
        });
        decor.requestApplyInsets();
        decor.post(this::applyImmersiveMode);
    }

    /** Hide Android status/navigation bars while preserving transient edge-swipe access. */
    private void applyImmersiveMode() {
        View decor = getWindow().getDecorView();
        if (android.os.Build.VERSION.SDK_INT >= 30) {
            WindowInsetsController controller = getWindow().getInsetsController();
            if (controller != null) {
                controller.setSystemBarsBehavior(WindowInsetsController.BEHAVIOR_SHOW_TRANSIENT_BARS_BY_SWIPE);
                controller.hide(WindowInsets.Type.systemBars());
            }
        } else {
            decor.setSystemUiVisibility(
                View.SYSTEM_UI_FLAG_FULLSCREEN
                | View.SYSTEM_UI_FLAG_HIDE_NAVIGATION
                | View.SYSTEM_UI_FLAG_IMMERSIVE_STICKY
                | View.SYSTEM_UI_FLAG_LAYOUT_STABLE
                | View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN
                | View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION);
        }
    }

    @Override protected void onResume() {
        super.onResume();
        applyImmersiveMode();
    }

    @Override public void onWindowFocusChanged(boolean hasFocus) {
        super.onWindowFocusChanged(hasFocus);
        if (hasFocus) applyImmersiveMode();
    }

    public int[] getSafeInsets() { return new int[]{insetLeft, insetTop, insetRight, insetBottom}; }
    private String saveDocumentId = "", saveName = "Untitled.psd";

    public boolean hasPendingRequest() { return pending; }
    public String pollResult() { String result = results.poll(); return result == null ? "" : result; }

    public void requestOpen() {
        if (pending) { emit("error", "", "", "", "A file request is already open"); return; }
        pending = true;
        runOnUiThread(() -> {
            try {
                Intent intent = new Intent(Intent.ACTION_OPEN_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                // PSD/PCraft providers often identify documents as application/octet-stream.
                intent.setType("*/*");
                intent.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION);
                startActivityForResult(intent, OPEN);
            } catch (RuntimeException e) { pending = false; emit("error", "", "", "", e.toString()); }
        });
    }
    public void requestSave(String documentId, String name) {
        if (pending) { emit("save", documentId, "", "", "A file request is already open"); return; }
        pending = true; saveDocumentId = documentId; saveName = safeName(name);
        runOnUiThread(() -> {
            try {
                Intent intent = new Intent(Intent.ACTION_CREATE_DOCUMENT);
                intent.addCategory(Intent.CATEGORY_OPENABLE);
                intent.setType(mime(saveName));
                intent.putExtra(Intent.EXTRA_TITLE, saveName);
                intent.addFlags(Intent.FLAG_GRANT_WRITE_URI_PERMISSION | Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
                startActivityForResult(intent, SAVE);
            } catch (RuntimeException e) { pending = false; emit("save", saveDocumentId, "", "", e.toString()); }
        });
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request != OPEN && request != SAVE) return;
        if (result != RESULT_OK || data == null || data.getData() == null) {
            pending = false;
            emit(request == SAVE ? "save" : "cancel", saveDocumentId, "", "", "");
            return;
        }
        Uri uri = data.getData();
        if (request == SAVE) {
            String name = displayName(uri, saveName);
            try {
                getContentResolver().takePersistableUriPermission(uri, data.getFlags() &
                    (Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_WRITE_URI_PERMISSION));
            } catch (RuntimeException ignored) { /* Immediate activity grant remains valid. */ }
            emit("save", saveDocumentId, uri.toString() + "#" + safeName(name), name, "");
            pending = false;
        } else {
            new Thread(() -> {
                File cached = null;
                try {
                    String name = displayName(uri, "image.png");
                    cached = File.createTempFile("photocraft-open-", ".tmp", getCacheDir());
                    try (InputStream input = getContentResolver().openInputStream(uri);
                         OutputStream output = new FileOutputStream(cached)) {
                        if (input == null) throw new java.io.IOException("The file provider returned no data");
                        byte[] buffer = new byte[65536]; long total = 0; int n;
                        while ((n = input.read(buffer)) != -1) {
                            total += n;
                            if (total > MAX_IMPORT_BYTES) throw new java.io.IOException("File exceeds the 256 MiB mobile import limit");
                            output.write(buffer, 0, n);
                        }
                    }
                    emit("open", "", cached.getAbsolutePath(), name, "");
                } catch (Exception e) {
                    if (cached != null) cached.delete();
                    emit("error", "", "", "", e.toString());
                } finally { pending = false; }
            }, "PhotoCraft-file-import").start();
        }
    }
    /** Called on the native editor thread, never on Android's UI thread. */
    public String writeDocument(String target, byte[] bytes) {
        try {
            int fragment = target.lastIndexOf('#');
            if (fragment < 0 || !target.startsWith("content://")) return "Invalid document destination";
            Uri uri = Uri.parse(target.substring(0, fragment));
            try (OutputStream output = getContentResolver().openOutputStream(uri, "wt")) {
                if (output == null) return "The file provider returned no output stream";
                output.write(bytes); output.flush();
            }
            return "";
        } catch (Exception e) { return e.toString(); }
    }
    private String displayName(Uri uri, String fallback) {
        try (Cursor cursor = getContentResolver().query(uri, new String[]{OpenableColumns.DISPLAY_NAME}, null, null, null)) {
            if (cursor != null && cursor.moveToFirst()) {
                String name = cursor.getString(0); if (name != null && !name.isEmpty()) return safeName(name);
            }
        } catch (RuntimeException ignored) { }
        return safeName(fallback);
    }
    private static String safeName(String name) {
        String safe = name.replaceAll("[\\\\/#\\x00-\\x1f]", "_");
        return safe.isEmpty() ? "Untitled.psd" : safe;
    }
    private static String mime(String name) {
        String n = name.toLowerCase(java.util.Locale.ROOT);
        if (n.endsWith(".png")) return "image/png";
        if (n.endsWith(".jpg") || n.endsWith(".jpeg")) return "image/jpeg";
        if (n.endsWith(".psd")) return "image/vnd.adobe.photoshop";
        return "application/octet-stream";
    }
    private void emit(String kind, String document, String path, String name, String error) {
        try {
            JSONObject item = new JSONObject();
            item.put("kind", kind); item.put("document", document); item.put("path", path);
            item.put("name", name); item.put("error", error); results.add(item.toString());
        } catch (org.json.JSONException e) { android.util.Log.e("PhotoCraft", "Could not report file result", e); }
    }
}
