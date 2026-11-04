import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.ConnectException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.UnknownHostException;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CancellationException;
import java.util.concurrent.CompletableFuture;
import java.util.concurrent.TimeUnit;

/**
 * Mã giả 5.1 – Các hàm gửi, nhận khung JSON "bất đồng bộ" theo cách của Java (Mục 5.2.3, 5.2.4).
 * Theo bảng ánh xạ của giáo trình, Java dùng mã chặn chạy trên luồng ảo (JDK 21): khi chờ vào/ra,
 * môi trường chạy tự tháo luồng ảo khỏi luồng hệ điều hành, nên ASYNC/AWAIT không cần cú pháp riêng.
 * Tín hiệu hủy ct là một CompletableFuture&lt;Void&gt;: ct hoàn tất nghĩa là đã hủy. Lời gọi chặn được
 * đánh thức bằng cách đóng socket (linkClose); khi đó hàm nhận ném CancellationException (Cancelled).
 * Kế thừa: Mã giả 3.3 (SEND_ALL), 3.4 (RECV_EXACT), 3.6 (khung JSON), 2.1, 2.2.
 */
final class Khung {
    static final int MAX_FRAME = 1048576;                               // 1 MiB

    /** Đối tác đóng kết nối; received = số byte của khung đã nhận được. */
    static class ConnectionClosed extends IOException {
        final int received;
        ConnectionClosed(int received) { super("Kết nối đã đóng (đã nhận " + received + " byte)"); this.received = received; }
    }

    /** Vi phạm giao thức, ví dụ khung quá lớn. */
    static class ProtocolError extends IOException {
        ProtocolError(String m) { super(m); }
    }

    /** Lỗi mạng đã phân loại (Mã giả 2.2): code, retry. */
    static class NetError extends IOException {
        final String code;
        final boolean retry;
        NetError(String code, boolean retry, Exception cause) {
            super(cause == null ? code : code + ": " + cause.getMessage(), cause);
            this.code = code; this.retry = retry;
        }
    }

    private Khung() {}

    /** Tín hiệu hủy chưa kích hoạt; cancelAfter(ms) tự kích hoạt sau ms mili giây (CANCEL_AFTER). */
    static CompletableFuture<Void> newCancellation() { return new CompletableFuture<>(); }
    static CompletableFuture<Void> cancelAfter(long ms) {
        return new CompletableFuture<Void>().completeOnTimeout(null, ms, TimeUnit.MILLISECONDS);
    }
    static void cancel(CompletableFuture<Void> ct) { ct.complete(null); }

    /** Khi ct bị hủy thì đóng socket, để lời gọi chặn trên socket kết thúc ngay. */
    static void linkClose(CompletableFuture<Void> ct, Socket s) {
        ct.thenRun(() -> { try { s.close(); } catch (IOException ignored) { } });
    }

    static byte[] makeFrame(Object obj) {                              // UINT32_BE(LENGTH(p)) + p
        byte[] p = Json.stringify(obj).getBytes(StandardCharsets.UTF_8);
        return ByteBuffer.allocate(4 + p.length).putInt(p.length).put(p).array();  // big-endian
    }

    /** Mã giả 5.1, dòng 1–11. */
    static byte[] recvExact(Socket s, int n, CompletableFuture<Void> ct) throws IOException {
        InputStream in = s.getInputStream();
        byte[] buf = new byte[n];
        int got = 0;
        while (got < n) {
            if (ct.isDone()) throw new CancellationException("Đã hủy");
            int k;
            try {
                k = in.read(buf, got, n - got);                        // RECV: luồng ảo chờ, không chiếm luồng HĐH
            } catch (IOException e) {
                if (ct.isDone()) throw new CancellationException("Đã hủy");   // socket bị đóng do hủy
                throw e;
            }
            if (k == -1) {
                if (ct.isDone()) throw new CancellationException("Đã hủy");
                throw new ConnectionClosed(got);                       // đối tác đã đóng kết nối
            }
            got += k;
        }
        return buf;
    }

    /** Mã giả 5.1, dòng 12–21. */
    static Object recvJson(Socket s, CompletableFuture<Void> ct) throws IOException {
        long n = ByteBuffer.wrap(recvExact(s, 4, ct)).getInt() & 0xFFFFFFFFL;  // FROM_UINT32_BE (không dấu)
        if (n > MAX_FRAME) throw new ProtocolError("Khung quá lớn");
        byte[] data;
        try {
            data = recvExact(s, (int) n, ct);
        } catch (ConnectionClosed e) {                                 // đã nhận xong tiêu đề
            throw new ConnectionClosed(4 + e.received);
        }
        return Json.parse(new String(data, StandardCharsets.UTF_8));
    }

    /** Mã giả 5.1, dòng 22–25: OutputStream.write của socket gửi hết mảng byte (SEND_ALL). */
    static void sendJson(Socket s, Object obj, CompletableFuture<Void> ct) throws IOException {
        if (ct.isDone()) throw new CancellationException("Đã hủy");
        OutputStream out = s.getOutputStream();
        try {
            out.write(makeFrame(obj));
            out.flush();
        } catch (IOException e) {
            if (ct.isDone()) throw new CancellationException("Đã hủy");
            throw e;
        }
    }

    /** Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định. */
    static Object[] parseEndpoint(String[] args, String defaultHost, int defaultPort) {
        String host = defaultHost;
        String port = System.getenv().getOrDefault("APP_PORT", String.valueOf(defaultPort));
        if (args.length >= 1) host = args[0];
        if (args.length >= 2) port = args[1];
        int p;
        try { p = Integer.parseInt(port); }
        catch (NumberFormatException e) { throw new IllegalArgumentException("Cổng phải là số nguyên: " + port); }
        if (p < 1 || p > 65535) throw new IllegalArgumentException("Cổng không hợp lệ: " + p);
        return new Object[] {host, p};
    }

    /** SAFE_CONNECT_ASYNC = Mã giả 2.2: trên luồng ảo, CONNECT chặn không chiếm luồng hệ điều hành. */
    static Socket safeConnect(String host, int port, int timeoutMs) throws NetError {
        InetAddress[] addrs;
        try { addrs = InetAddress.getAllByName(host); }                 // RESOLVE
        catch (UnknownHostException e) { throw new NetError("NOT_FOUND", false, e); }
        int refused = 0;
        IOException last = null;
        for (InetAddress a : addrs) {
            Socket s = new Socket();
            try {
                s.connect(new InetSocketAddress(a, port), timeoutMs);  // CONNECT_TIMEOUT
                return s;
            } catch (ConnectException e) {                             // nhận RST: bị từ chối
                refused++; last = e; closeQuietly(s);
            } catch (IOException e) {                                  // hết giờ, mạng không tới được
                last = e; closeQuietly(s);
            }
        }
        if (refused > 0 && refused == addrs.length) throw new NetError("REFUSED", false, last);
        throw new NetError("UNREACHABLE", true, last);
    }

    static void closeQuietly(Socket s) {
        try { s.close(); } catch (IOException ignored) { }
    }
}
