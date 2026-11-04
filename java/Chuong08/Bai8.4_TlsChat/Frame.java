import java.io.DataInputStream;
import java.io.DataOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.nio.charset.StandardCharsets;
import java.util.Map;

/**
 * Bài 8.4 – Gửi, nhận khung JSON (Mã giả 3.6 / 5.1): 4 byte độ dài big-endian + JSON UTF-8.
 * Chạy trên SSLSocket giống hệt trên Socket gốc: định khung không đổi khi thêm TLS (Mục 8.4.5).
 */
public class Frame {
    static final int MAX_FRAME = 1048576;                              // 1 MiB

    @SuppressWarnings("unchecked")
    public static Map<String, Object> recvJson(InputStream in) throws IOException {
        DataInputStream d = new DataInputStream(in);
        long n = d.readInt() & 0xFFFFFFFFL;                            // FROM_UINT32_BE (không dấu); đóng -> EOFException
        if (n > MAX_FRAME) throw new IOException("Khung quá lớn");     // ProtocolError
        byte[] data = new byte[(int) n];
        d.readFully(data);                                             // RECV_EXACT(s, n)
        Object o;
        try { o = Json.parse(new String(data, StandardCharsets.UTF_8)); }
        catch (IllegalArgumentException e) { throw new IOException("JSON lỗi: " + e.getMessage()); }
        if (!(o instanceof Map)) throw new IOException("Không phải đối tượng JSON");
        return (Map<String, Object>) o;
    }

    public static void sendJson(OutputStream out, Map<String, Object> obj) throws IOException {
        byte[] p = Json.stringify(obj).getBytes(StandardCharsets.UTF_8);
        DataOutputStream d = new DataOutputStream(out);
        byte[] frame = new byte[4 + p.length];                         // ghép tiêu đề với nội dung rồi gửi
        frame[0] = (byte) (p.length >>> 24); frame[1] = (byte) (p.length >>> 16);
        frame[2] = (byte) (p.length >>> 8);  frame[3] = (byte) p.length;
        System.arraycopy(p, 0, frame, 4, p.length);
        d.write(frame);                                                // SEND_ALL
        d.flush();
    }

    static String str(Object o) { return o instanceof String s ? s : null; }
}
