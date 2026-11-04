import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.function.BiConsumer;
import java.util.function.Consumer;

/**
 * Bài 8.1 – Mã giả 8.1 STREAM_COMPLETION, kế thừa Mã giả 7.5 SSE_READ (Mục 8.2.2, 7.3.3).
 * Lớp được LlmClient dùng; không chạy riêng.
 */
public class LlmStream {
    /** Biến trạng thái st của Mã giả 8.1. */
    public static class State {
        public String text = "";
        public Map<String, Object> usage;
        public boolean done;
    }

    /** READ_LINE (Mã giả 3.5) trên thân phản hồi; HttpClient đã giải mã khối (chunked).
     *  Trả về null khi thân kết thúc hoặc kết nối bị ngắt (SSE_READ chỉ dừng ở hai trường hợp này). */
    static String readLine(BufferedReader r, int maxLen) throws NetError {
        String line;
        try {
            line = r.readLine();
        } catch (IOException e) {                  // ngắt kết nối, khối bị cắt dở
            return null;
        }
        if (line != null && line.length() > maxLen) throw new NetError("Dòng SSE quá dài");
        return line;
    }

    /** Mã giả 7.5 – SSE_READ(r, on_event). */
    static void sseRead(BufferedReader r, BiConsumer<String, String> onEvent) throws NetError {
        String type = "message";
        List<String> data = new ArrayList<>();
        String line;
        while ((line = readLine(r, 65536)) != null) {
            if (line.isEmpty()) {                                  // dòng trống: hết một sự kiện
                if (!data.isEmpty()) onEvent.accept(type, String.join("\n", data));
                type = "message";
                data = new ArrayList<>();
            } else if (!line.startsWith(":")) {                     // ":": dòng chú thích
                int i = line.indexOf(':');
                String field = i < 0 ? line : line.substring(0, i);
                String value = i < 0 ? "" : line.substring(i + 1);
                if (value.startsWith(" ")) value = value.substring(1);   // STRIP_ONE_SPACE
                if (field.equals("event")) type = value;
                else if (field.equals("data")) data.add(value);
                // "id", "retry": không dùng trong bài này
            }
        }                                                          // null: server đóng luồng
    }

    /** Mã giả 8.1 – STREAM_COMPLETION(r, on_token); body: thân phản hồi sau tiêu đề. */
    @SuppressWarnings("unchecked")
    public static State streamCompletion(InputStream body, Consumer<String> onToken) throws NetError {
        State st = new State();                                               // dòng 1
        BiConsumer<String, String> onEvent = (type, data) -> {                // dòng 2
            if (data.equals("[DONE]")) { st.done = true; return; }            // dòng 3
            Map<String, Object> chunk = (Map<String, Object>) Json.parse(data);   // dòng 4
            if (chunk.get("usage") != null)                                   // dòng 5–6
                st.usage = (Map<String, Object>) chunk.get("usage");
            List<Object> choices = (List<Object>) chunk.get("choices");
            if (choices == null || choices.isEmpty()) return;                 // dòng 7
            Map<String, Object> delta = (Map<String, Object>) ((Map<String, Object>) choices.get(0)).get("delta");
            String d = delta == null ? null : (String) delta.get("content");  // dòng 8
            if (d != null) {                                                  // dòng 9–12
                st.text += d;
                onToken.accept(d);
            }
        };
        BufferedReader r = new BufferedReader(new InputStreamReader(body, StandardCharsets.UTF_8));
        sseRead(r, onEvent);                                                  // dòng 14
        if (!st.done) throw new NetError("Luồng bị ngắt giữa chừng");          // dòng 15
        return st;                                                            // dòng 16
    }
}
