import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.List;
import java.util.Map;
import java.util.concurrent.ThreadLocalRandom;

/**
 * Bài mở rộng MR6.4 – Giao thức trò chơi Caro: khung JSON có tiền tố độ dài (chép từ MR3.3, Bài 3.2).
 *
 * Khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8. Mỗi thông điệp có trường "type":
 *   client -> server: hello {name}, list, create {room}, join {room}, watch {room}, move {r, c}, rematch, leave, bye
 *   server -> client: welcome, rooms, joined, start, board, moved, over, rematch_wait, left, room_closed,
 *                     error {code, msg}
 * Khám phá server trong LAN (UDP broadcast, như Bài 6.5):
 *   client phát {"type": "discover", "service": "caro", "id": n}
 *   server trả unicast {"id": n, "name", "port", "rooms", "players"}
 */
final class GiaoThuc {
    private GiaoThuc() {}

    static final int MAX_FRAME = 64 * 1024;           // thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ
    static final String DISC_SERVICE = "caro";

    /** Mã giả 3.4 (RECV_EXACT): nhận đúng n byte. */
    static byte[] recvExact(InputStream in, int n) throws IOException {
        byte[] b = in.readNBytes(n);
        if (b.length < n) throw new EOFException("kết nối đã đóng");
        return b;
    }

    /** Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi trong một lần write. */
    static void sendFrame(Socket s, byte[] payload) throws IOException {
        byte[] f = ByteBuffer.allocate(4 + payload.length).putInt(payload.length).put(payload).array();
        synchronized (s) { s.getOutputStream().write(f); }
    }

    /** Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte. */
    static byte[] recvFrame(InputStream in) throws IOException {
        long n = ByteBuffer.wrap(recvExact(in, 4)).getInt() & 0xFFFFFFFFL;
        if (n > MAX_FRAME) throw new IOException("Khung quá lớn: " + n + " byte");
        return recvExact(in, (int) n);
    }

    static void sendJSON(Socket s, Map<String, Object> obj) {
        try {
            sendFrame(s, Json.stringify(obj).getBytes(StandardCharsets.UTF_8));
        } catch (IOException ignored) { }                // đối tác đã mất: luồng đọc sẽ báo
    }

    /** Giải mã nội dung khung; null nếu không phải đối tượng JSON hợp lệ. */
    @SuppressWarnings("unchecked")
    static Map<String, Object> decodeJSON(byte[] payload, int len) {
        try {
            String text = StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(payload, 0, len)).toString();
            Object o = Json.parse(text);
            return o instanceof Map<?, ?> ? (Map<String, Object>) o : null;
        } catch (Exception e) {
            return null;
        }
    }

    static Map<String, Object> recvJSON(InputStream in) throws IOException {
        byte[] p = recvFrame(in);
        return decodeJSON(p, p.length);
    }

    /** Trường chuỗi của thông điệp ("" nếu thiếu hoặc sai kiểu). */
    static String str(Map<String, Object> m, String key) { return m != null && m.get(key) instanceof String s ? s : ""; }

    /** Trường số nguyên (0 nếu thiếu hoặc sai kiểu). */
    static int num(Map<String, Object> m, String key) { return m != null && m.get(key) instanceof Long n ? n.intValue() : 0; }

    /** Giá trị JSON có phải số nguyên không (true/false, chuỗi, 50.5 đều không phải); Json.java trả số nguyên về Long. */
    static boolean isInt(Object v) { return v instanceof Long; }

    /** In giá trị giống repr() của Python để nhật ký các bản trùng nhau. */
    static String repr(Object v) {
        if (v == null) return "None";
        if (v instanceof String s) return "'" + s + "'";
        if (v instanceof Boolean b) return b ? "True" : "False";
        return v.toString();
    }

    record Found(String ip, int port, String name, int rooms, int players) {}

    /**
     * Mã giả 6.7 (DISCOVER, Bài 6.5): phát một yêu cầu broadcast, thu thập MỌI phản hồi đến hết thời gian chờ.
     * Danh sách được sắp theo tên để kết quả ổn định.
     */
    static List<Found> discover(int discPort, int waitMs, String bcast) throws IOException {
        List<Found> list = new ArrayList<>();
        try (DatagramSocket s = new DatagramSocket(new InetSocketAddress("0.0.0.0", 0))) {
            s.setBroadcast(true);                          // SO_BROADCAST: bắt buộc để gửi tới địa chỉ broadcast
            int qid = ThreadLocalRandom.current().nextInt(999999) + 1;
            byte[] q = Json.stringify(Json.obj("type", "discover", "service", DISC_SERVICE, "id", qid)).getBytes(StandardCharsets.UTF_8);
            s.send(new DatagramPacket(q, q.length, new InetSocketAddress(bcast, discPort)));
            long deadline = System.nanoTime() + waitMs * 1_000_000L;
            byte[] buf = new byte[2048];
            while (true) {
                long left = (deadline - System.nanoTime()) / 1_000_000;
                if (left <= 0) break;
                s.setSoTimeout((int) left);
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                try {
                    s.receive(p);
                } catch (SocketTimeoutException e) {
                    break;                                 // hết thời gian chờ
                }
                Map<String, Object> r = decodeJSON(p.getData(), p.getLength());
                if (r == null || num(r, "id") != qid) continue;   // chỉ nhận phản hồi đúng yêu cầu của mình
                list.add(new Found(p.getAddress().getHostAddress(), num(r, "port"), str(r, "name"), num(r, "rooms"), num(r, "players")));
            }
        }
        list.sort(Comparator.comparing(Found::name).thenComparing(Found::ip).thenComparing(f -> String.valueOf(f.port())));
        return list;
    }
}
