import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài 5.4 – Giao diện dòng lệnh dùng lớp ChatClient (Mã giả 5.5): chỉ đăng ký onMessage, onStatus và
 * gọi sendMessage, disconnect. Mỗi dòng nhập là một tin; /quit để thoát; /cho &lt;giây&gt; tạm dừng đọc
 * bàn phím (dùng trong kịch bản minh họa). Vòng nhận chạy trên luồng ảo riêng nên tin đến được hiển thị
 * ngay cả khi luồng chính đang chờ người dùng gõ.
 * Chạy: java -cp out ChatCli &lt;tên&gt; [host] [port]      (mặc định 127.0.0.1 9200)
 */
public class ChatCli {
    static final String ROOM = "chung";                             // phòng đang chọn; server tối giản chỉ có "chung"

    static void showMessage(String me, Map<?, ?> msg) {             // trình xử lý sự kiện onMessage
        Object t = msg.get("type");
        if ("msg".equals(t)) System.out.println("[" + me + "] " + msg.get("from") + ": " + msg.get("text"));
        else if ("sys".equals(t)) System.out.println("[" + me + "] (hệ thống) " + msg.get("text"));
        else if ("welcome".equals(t))
            System.out.println("[" + me + "] Server chào mừng; đang trực tuyến: " + String.join(", ", ((List<?>) msg.get("users")).stream().map(String::valueOf).toList()));
        else if ("error".equals(t)) System.out.println("[" + me + "] Lỗi " + msg.get("code") + ": " + msg.get("text"));
        else System.out.println("[" + me + "] Nhận: " + Json.stringify(msg));
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 1) { System.out.println("Cách dùng: java -cp out ChatCli <tên> [host] [port]"); return; }
        String me = args[0];
        Object[] ep = Khung.parseEndpoint(java.util.Arrays.copyOfRange(args, 1, args.length), "127.0.0.1", 9200);
        ChatClient c = new ChatClient();
        c.onMessage = msg -> showMessage(me, msg);                   // đăng ký sự kiện
        c.onStatus = text -> System.out.println("[" + me + "] Trạng thái: " + text);
        try {
            c.connect((String) ep[0], (Integer) ep[1], me);
        } catch (IOException e) {
            System.out.println("[" + me + "] Không kết nối được: " + e.getMessage());
            return;
        }
        AtomicBoolean done = new AtomicBoolean();
        Runnable finish = () -> {
            if (done.compareAndSet(false, true)) { c.disconnect(); System.out.println("[" + me + "] Đã ngắt kết nối"); }
        };
        Runtime.getRuntime().addShutdownHook(new Thread(finish));    // Ctrl+C: thoát gọn
        BufferedReader console = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        try {
            String line;
            while ((line = console.readLine()) != null && !line.equals("/quit")) {
                if (line.startsWith("/cho ")) Thread.sleep((long) (Double.parseDouble(line.substring(5)) * 1000));
                else if (!line.isEmpty()) c.sendMessage(ROOM, line); // SEND_MESSAGE(c, room, text)
            }
        } finally {
            finish.run();
        }
    }
}
