import java.io.DataInputStream;
import java.io.IOException;
import java.io.OutputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.Map;

/** Các thủ tục kế thừa dùng chung của Bài 6.2 (chép gọn từ Chương 2, 3). */
final class NetUtil {
    private NetUtil() {}

    static final int MAX_FRAME = 1048576;                               // Mã giả 3.6: 1 MiB

    /** Đối tác đóng kết nối; received = số byte của khung đang nhận dở. */
    static final class ConnectionClosed extends IOException {
        final int received;
        ConnectionClosed(int received) {
            super("Kết nối bị đóng sau " + received + " byte");
            this.received = received;
        }
    }

    /** Vi phạm định dạng khung (ví dụ khung quá lớn). */
    static final class ProtocolError extends IOException {
        ProtocolError(String m) { super(m); }
    }

    /** Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > mặc định. */
    static Object[] parseEndpoint(String[] args, String defaultHost, int defaultPort) {
        String host = System.getenv().getOrDefault("APP_HOST", defaultHost);
        String port = System.getenv().getOrDefault("APP_PORT", String.valueOf(defaultPort));
        if (args.length >= 1) host = args[0];
        if (args.length >= 2) port = args[1];
        int p;
        try {
            p = Integer.parseInt(port);
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("Cổng phải là số nguyên: " + port);
        }
        if (p < 1 || p > 65535) throw new IllegalArgumentException("Cổng không hợp lệ: " + p);
        return new Object[] {host, p};
    }

    /** Đọc tham số nguyên thứ i (nếu có), ngược lại dùng giá trị mặc định. */
    static int intArg(String[] args, int i, int def) {
        return args.length > i ? Integer.parseInt(args[i]) : def;
    }

    /** Mã giả 2.2 (rút gọn) – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối. */
    static Socket safeConnect(String host, int port, int timeoutMs) throws IOException {
        IOException last = null;
        for (InetAddress ip : InetAddress.getAllByName(host)) {
            Socket s = new Socket();
            try {
                s.connect(new InetSocketAddress(ip, port), timeoutMs);
                return s;
            } catch (IOException e) {
                last = e;
                s.close();
            }
        }
        throw new IOException("Không kết nối được " + host + ":" + port + ": " + (last == null ? "" : last.getMessage()));
    }

    /** Mã giả 3.4 – RECV_EXACT: đọc lặp đến khi đủ n byte (tương đương readFully nhưng biết số byte đã nhận). */
    static byte[] recvExact(DataInputStream in, int n) throws IOException {
        byte[] buf = new byte[n];
        int got = 0;
        while (got < n) {
            int k = in.read(buf, got, n - got);
            if (k == -1) throw new ConnectionClosed(got);
            got += k;
        }
        return buf;
    }

    /** Mã giả 3.6 – SEND_JSON: đối tượng → JSON → UTF-8 → khung 4 byte độ dài (big-endian); write gửi hết. */
    static void sendJson(OutputStream out, Map<String, Object> obj) throws IOException {
        byte[] payload = Json.encode(obj).getBytes(StandardCharsets.UTF_8);
        byte[] frame = ByteBuffer.allocate(4 + payload.length).putInt(payload.length).put(payload).array();
        out.write(frame);                                                // SEND_ALL
        out.flush();
    }

    /** Mã giả 3.6 – RECV_JSON (gồm RECV_FRAME). JSON sai gây Json.JsonError. */
    static Map<String, Object> recvJson(DataInputStream in) throws IOException {
        long n = ByteBuffer.wrap(recvExact(in, 4)).getInt() & 0xFFFFFFFFL;   // FROM_UINT32_BE
        if (n > MAX_FRAME) throw new ProtocolError("Khung quá lớn: " + n);
        byte[] data;
        try {
            data = recvExact(in, (int) n);
        } catch (ConnectionClosed e) {                                   // đã nhận xong tiêu đề
            throw new ConnectionClosed(4 + e.received);
        }
        return Json.decodeObject(new String(data, StandardCharsets.UTF_8));
    }

    /** Chuỗi "ip:cổng" của đầu bên kia, dùng để ghi nhật ký. */
    static String addr(Socket s) {
        return s.getInetAddress().getHostAddress() + ":" + s.getPort();
    }
}
