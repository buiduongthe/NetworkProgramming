import java.io.IOException;
import java.io.InputStream;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.charset.CharacterCodingException;
import java.nio.charset.StandardCharsets;
import java.util.Arrays;

/**
 * Thư viện định khung kế thừa từ Bài 3.2 (chép nguyên để bài tự chứa): Mã giả 3.3 (SEND_ALL), 3.4 (RECV_EXACT), 3.5 (READ_LINE),
 * 3.6 (SEND_FRAME, RECV_FRAME, SEND_JSON, RECV_JSON) – Mục 3.2.4, 3.4.3 Giáo trình Lập trình mạng.
 * Khung = 4 byte độ dài (big-endian, không dấu) + nội dung; thông điệp JSON mã hóa UTF-8.
 */
final class Framing {
    private Framing() {}

    static final int MAX_FRAME = 1048576;                                  // 1 MiB

    /** Đối tác đóng kết nối; received = số byte đã nhận của khung (hoặc tệp) dở dang. */
    static final class ConnectionClosed extends IOException {
        final int received;

        ConnectionClosed(int received) { this(received, "Kết nối đóng giữa chừng (đã nhận " + received + " byte)"); }

        ConnectionClosed(int received, String message) { super(message); this.received = received; }
    }

    /** Đối tác vi phạm định dạng giao thức (khung quá lớn, dòng quá dài...). */
    static final class ProtocolError extends IOException {
        ProtocolError(String m) { super(m); }
    }

    /** Mã giả 3.5 – STRUCT Reader { sock, pending }: bộ đệm tồn tại giữa các lần READ_LINE. */
    static final class Reader {
        final Socket sock;
        byte[] pending = new byte[0];

        Reader(Socket sock) { this.sock = sock; }
    }

    /** Mã giả 3.3 – SEND_ALL: OutputStream.write() của socket Java đã lặp bên trong đến khi gửi hết. */
    static void sendAll(Socket s, byte[] data) throws IOException {
        s.getOutputStream().write(data);
        s.getOutputStream().flush();
    }

    /** Mã giả 3.4 – RECV_EXACT: nhận đúng n byte; báo ConnectionClosed kèm số byte đã nhận. */
    static byte[] recvExact(Socket s, int n) throws IOException {
        InputStream in = s.getInputStream();
        byte[] buf = new byte[n];
        int got = 0;
        while (got < n) {
            int k = in.read(buf, got, n - got);                            // RECV: -1 khi đối tác đã đóng
            if (k == -1) throw new ConnectionClosed(got);
            got += k;
        }
        return buf;
    }

    /** Mã giả 3.5 – READ_LINE: đọc một dòng kết thúc bằng LF hoặc CRLF; trả null khi đối tác đã đóng. */
    static String readLine(Reader r, int maxLen) throws IOException {
        byte[] chunk = new byte[4096];
        while (true) {
            int i = indexOf(r.pending, (byte) 0x0A);                       // vị trí '\n'
            if (i > maxLen) throw new ProtocolError("Dòng quá dài");        // dòng 5: có LF nhưng quá dài
            if (i >= 0) {
                int len = (i > 0 && r.pending[i - 1] == 0x0D) ? i - 1 : i; // STRIP_CR
                String line = decodeUtf8(Arrays.copyOfRange(r.pending, 0, len));
                r.pending = Arrays.copyOfRange(r.pending, i + 1, r.pending.length);
                return line;
            }
            if (r.pending.length > maxLen) throw new ProtocolError("Dòng quá dài");
            int k = r.sock.getInputStream().read(chunk);
            if (k == -1) return null;                                      // đối tác đã đóng
            byte[] merged = Arrays.copyOf(r.pending, r.pending.length + k);
            System.arraycopy(chunk, 0, merged, r.pending.length, k);
            r.pending = merged;
        }
    }

    private static int indexOf(byte[] a, byte b) {
        for (int i = 0; i < a.length; i++) if (a[i] == b) return i;
        return -1;
    }

    /** Mã giả 3.6 – SEND_FRAME: ghép 4 byte độ dài (big-endian) với nội dung rồi gửi một lần. */
    static void sendFrame(Socket s, byte[] payload) throws IOException {
        ByteBuffer frame = ByteBuffer.allocate(4 + payload.length);       // ByteBuffer mặc định big-endian
        frame.putInt(payload.length).put(payload);                         // UINT32_BE(LENGTH(payload)) + payload
        sendAll(s, frame.array());
    }

    /** Mã giả 3.6 – RECV_FRAME: nhận đủ tiêu đề, kiểm tra MAX_FRAME, nhận đúng n byte nội dung. */
    static byte[] recvFrame(Socket s) throws IOException {
        long n = ByteBuffer.wrap(recvExact(s, 4)).getInt() & 0xFFFFFFFFL;  // FROM_UINT32_BE (không dấu)
        if (n > MAX_FRAME) throw new ProtocolError("Khung quá lớn: " + n);
        try {
            return recvExact(s, (int) n);
        } catch (ConnectionClosed e) {                                     // đã nhận xong tiêu đề
            throw new ConnectionClosed(4 + e.received);
        }
    }

    /** DECODE(…, "UTF-8") nghiêm ngặt: byte sai báo lỗi thay vì thay bằng ký tự '?'. */
    static String decodeUtf8(byte[] b) {
        try {
            return StandardCharsets.UTF_8.newDecoder().decode(ByteBuffer.wrap(b)).toString();
        } catch (CharacterCodingException e) {
            throw new Json.JsonError("Không phải UTF-8 hợp lệ");
        }
    }

    /** DECODE rồi JSON_DECODE; mọi lỗi giải mã đều quy về Json.JsonError. */
    static Object jsonDecode(byte[] payload) { return Json.parse(decodeUtf8(payload)); }

    static void sendJson(Socket s, Object obj) throws IOException {        // Mã giả 3.6, SEND_JSON
        sendFrame(s, Json.stringify(obj).getBytes(StandardCharsets.UTF_8));
    }

    static Object recvJson(Socket s) throws IOException {                   // Mã giả 3.6, RECV_JSON
        return jsonDecode(recvFrame(s));
    }
}
