import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR3.3 – Giao thức trò chơi đoán số: khung JSON có tiền tố độ dài, bộ sinh số giả ngẫu nhiên.
 *
 * Kế thừa Bài 3.2 (Mã giả 3.3–3.6): khung = 4 byte độ dài (big-endian) + nội dung JSON mã hóa UTF-8.
 * Các thông điệp (mỗi thông điệp là một đối tượng JSON có trường "type"):
 *   client -> server: hello {name}, guess {value}, bye
 *   server -> client: welcome, start, turn, result, timeout, left, error {code, msg}, game_over, bye
 */
final class GiaoThuc {
    private GiaoThuc() {}

    static final int MAX_FRAME = 64 * 1024;      // thông điệp trò chơi rất nhỏ: giới hạn 64 KiB chống khung khổng lồ

    /** Đối tác đóng kết nối. */
    static final class ConnectionClosed extends EOFException {
        ConnectionClosed() { super("đối tác đóng kết nối"); }
    }

    /** Đối tác vi phạm định dạng khung (khung quá lớn...). */
    static final class ProtocolError extends IOException {
        ProtocolError(String m) { super(m); }
    }

    /** Mã giả 3.4 (RECV_EXACT): nhận đúng n byte. */
    static byte[] recvExact(InputStream in, int n) throws IOException {
        byte[] buf = in.readNBytes(n);                       // lặp read() bên trong tới khi đủ n byte hoặc EOF
        if (buf.length < n) throw new ConnectionClosed();
        return buf;
    }

    /** Mã giả 3.6 (SEND_FRAME): 4 byte độ dài + nội dung, gửi hết trong một lần write (SEND_ALL). */
    static void sendFrame(Socket s, byte[] payload) throws IOException {
        ByteBuffer b = ByteBuffer.allocate(4 + payload.length);   // ByteBuffer mặc định big-endian
        b.putInt(payload.length).put(payload);
        OutputStream out = s.getOutputStream();
        synchronized (s) { out.write(b.array()); }               // một khung không bị xen bởi luồng khác
    }

    /** Mã giả 3.6 (RECV_FRAME): đọc tiêu đề, kiểm tra giới hạn, đọc đúng n byte. */
    static byte[] recvFrame(InputStream in) throws IOException {
        long n = ByteBuffer.wrap(recvExact(in, 4)).getInt() & 0xFFFFFFFFL;   // số không dấu 32 bit
        if (n > MAX_FRAME) throw new ProtocolError("Khung quá lớn: " + n + " byte");
        return recvExact(in, (int) n);
    }

    static void sendJson(Socket s, Map<String, Object> obj) throws IOException {
        sendFrame(s, Json.stringify(obj).getBytes(StandardCharsets.UTF_8));
    }

    /** Giải mã nội dung khung; trả null nếu không phải đối tượng JSON hợp lệ. */
    @SuppressWarnings("unchecked")
    static Map<String, Object> decodeJson(byte[] payload) {
        try {
            String text = StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(payload)).toString();  // báo lỗi nếu sai UTF-8
            Object o = Json.parse(text);
            return o instanceof Map<?, ?> ? (Map<String, Object>) o : null;
        } catch (Exception e) {                              // CharacterCodingException, lỗi cú pháp JSON
            return null;
        }
    }

    static Map<String, Object> recvJson(InputStream in) throws IOException {
        return decodeJson(recvFrame(in));
    }

    /** Biểu diễn giá trị giống repr() của Python, để nhật ký giống bản Python ('năm mươi', None, True). */
    static String repr(Object v) {
        if (v == null) return "None";
        if (v instanceof String str) return "'" + str + "'";
        if (v instanceof Boolean b) return b ? "True" : "False";
        if (v instanceof List<?> || v instanceof Map<?, ?>) return Json.stringify(v);
        return v.toString();
    }

    /**
     * Bộ sinh số giả ngẫu nhiên đồng dư tuyến tính (LCG) rất đơn giản.
     * Mọi bản ngôn ngữ dùng cùng công thức nên với cùng hạt giống (seed) cho cùng dãy số,
     * nhờ đó kết quả demo trùng nhau. Không dùng cho mục đích bảo mật.
     */
    static final class Lcg {
        private long x;
        Lcg(long seed) { x = seed & 0x7FFFFFFFL; }

        /** Số nguyên ngẫu nhiên trong [lo, hi]. */
        int nextInt(int lo, int hi) {
            x = (x * 1103515245L + 12345) & 0x7FFFFFFFL;
            return lo + (int) ((x >> 8) % (hi - lo + 1));
        }
    }
}
