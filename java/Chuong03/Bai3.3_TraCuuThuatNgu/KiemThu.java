import java.io.IOException;
import java.net.Socket;
import java.net.SocketException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;

/**
 * Bài 3.3 – Client kiểm thử server tra cứu theo tập ca kiểm thử tối thiểu (Mục 3.5.3).
 * Client tự tạo dãy byte gửi đi (không dùng client chuẩn) để thực hiện các ca 1–7, cộng một ca
 * bổ sung cho thao tác không có trong đặc tả. Mỗi ca mở một kết nối riêng tới server.
 * Chạy: java -cp out KiemThu [host] [port]      (mặc định 127.0.0.1 9100)
 */
public class KiemThu {
    static String host;
    static int port;
    static final List<Boolean> results = new ArrayList<>();

    static Socket connect() throws IOException {
        Socket s = NetUtil.safeConnect(host, port, 3000);
        s.setSoTimeout(5000);
        return s;
    }

    static byte[] frameOf(Object obj) {                                     // khung hoàn chỉnh của một đối tượng JSON
        byte[] data = Json.stringify(obj).getBytes(StandardCharsets.UTF_8);
        return ByteBuffer.allocate(4 + data.length).putInt(data.length).put(data).array();
    }

    static byte[] concat(byte[] a, byte[] b) {
        return ByteBuffer.allocate(a.length + b.length).put(a).put(b).array();
    }

    static void report(String no, String name, boolean ok, String detail) {
        results.add(ok);
        System.out.println("[kiểm thử] Ca " + no + " – " + name + ": " + (ok ? "ĐẠT" : "KHÔNG ĐẠT") + " (" + detail + ")");
    }

    static Map<?, ?> lookup(Socket s, String term) throws IOException {
        Framing.sendJson(s, Json.obj("op", "lookup", "term", term));
        return (Map<?, ?>) Framing.recvJson(s);
    }

    static long status(Map<?, ?> r) { return r.get("status") instanceof Long v ? v : -1; }

    static void bye(Socket s) throws IOException { Framing.sendJson(s, Json.obj("op", "bye")); }

    public static void main(String[] args) throws Exception {
        Object[] ep = NetUtil.parseEndpoint(args, "127.0.0.1", 9100);
        host = (String) ep[0];
        port = (Integer) ep[1];
        System.out.println("[kiểm thử] Server " + host + ":" + port);
        try (Socket s = connect()) {                                        // Ca 1, 2
            Map<?, ?> r = lookup(s, "TCP");
            report("1", "thuật ngữ có trong từ điển", status(r) == 200 && "TCP".equals(r.get("term")), "status " + status(r));
            r = lookup(s, "Bắt tay ba bước");
            report("2", "thuật ngữ có dấu", status(r) == 200 && "Bắt tay ba bước".equals(r.get("term")),
                    "status " + status(r) + ", term '" + r.get("term") + "'");
            bye(s);
        }
        try (Socket s = connect()) {                                        // Ca 3: gửi từng byte, cách nhau 100 ms
            s.setTcpNoDelay(true);
            byte[] data = frameOf(Json.obj("op", "lookup", "term", "UDP"));
            for (byte b : data) {
                Framing.sendAll(s, new byte[] {b});
                Thread.sleep(100);
            }
            Map<?, ?> r = (Map<?, ?>) Framing.recvJson(s);
            report("3", "gửi " + data.length + " byte từng byte một", status(r) == 200 && "UDP".equals(r.get("term")),
                    "status " + status(r) + ", term '" + r.get("term") + "'");
            bye(s);
        }
        try (Socket s = connect()) {                                        // Ca 4: hai yêu cầu trong một lần SEND
            Framing.sendAll(s, concat(frameOf(Json.obj("op", "lookup", "term", "TCP")),
                                      frameOf(Json.obj("op", "lookup", "term", "XYZ"))));
            Map<?, ?> r1 = (Map<?, ?>) Framing.recvJson(s);
            Map<?, ?> r2 = (Map<?, ?>) Framing.recvJson(s);
            report("4", "hai yêu cầu trong một lần SEND", status(r1) == 200 && status(r2) == 404,
                    "status " + status(r1) + " rồi " + status(r2));
            bye(s);
        }
        try (Socket s = connect()) {                                        // Ca 5: độ dài 0xFFFFFFFF > MAX_FRAME
            Framing.sendAll(s, new byte[] {(byte) 0xFF, (byte) 0xFF, (byte) 0xFF, (byte) 0xFF});
            boolean closed;
            try {
                closed = s.getInputStream().read() == -1;
            } catch (SocketException e) {                                   // Connection reset
                closed = true;
            }
            report("5", "độ dài 4294967295 > MAX_FRAME", closed, closed ? "server đóng kết nối" : "không đóng");
        }
        try (Socket s = connect()) {                                        // Ca 6: nội dung không phải JSON
            Framing.sendFrame(s, "đây không phải JSON".getBytes(StandardCharsets.UTF_8));
            Map<?, ?> r = (Map<?, ?>) Framing.recvJson(s);
            Framing.sendJson(s, Json.obj("op", "list"));                    // kết nối vẫn dùng được
            Map<?, ?> r2 = (Map<?, ?>) Framing.recvJson(s);
            int n = r2.get("terms") instanceof List<?> l ? l.size() : 0;
            report("6", "nội dung không phải JSON", status(r) == 400 && status(r2) == 200,
                    "status " + status(r) + ", sau đó list -> " + status(r2) + ", " + n + " thuật ngữ");
            Framing.sendJson(s, Json.obj("op", "xoa", "term", "TCP"));      // Ca bổ sung: thao tác sai
            r = (Map<?, ?>) Framing.recvJson(s);
            report("bổ sung", "thao tác \"xoa\"", status(r) == 400, "status " + status(r) + ": " + r.get("error"));
            Framing.sendFrame(s, "[1,2]".getBytes(StandardCharsets.UTF_8));  // Ca bổ sung: JSON là mảng
            r = (Map<?, ?>) Framing.recvJson(s);
            report("bổ sung", "JSON là mảng [1,2]", status(r) == 400, "status " + status(r) + ": " + r.get("error"));
            bye(s);
        }
        try (Socket s = connect()) {                                        // Ca 7: đóng kết nối giữa khung
            Framing.sendAll(s, concat(ByteBuffer.allocate(4).putInt(50).array(),
                                      "0123456789".getBytes(StandardCharsets.US_ASCII)));
        }
        try (Socket s = connect()) {
            Map<?, ?> r = lookup(s, "TCP");
            report("7", "đóng giữa khung (14/54 byte)", status(r) == 200, "client mới vẫn được phục vụ, status " + status(r));
            bye(s);
        }
        long ok = results.stream().filter(x -> x).count();
        System.out.println("[kiểm thử] Kết quả: " + ok + "/" + results.size() + " ca đạt");
    }
}
