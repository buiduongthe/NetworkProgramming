import java.io.IOException;
import java.net.ConnectException;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.UnknownHostException;

/** Tiện ích kết nối kế thừa từ Chương 2 (Mã giả 2.1, 2.2), chép gọn vào bài để bài tự chứa. */
final class NetUtil {
    private NetUtil() {}

    /** Lỗi mạng đã phân loại: kind ∈ {NOT_FOUND, REFUSED, UNREACHABLE}, retry = có nên thử lại. */
    static final class NetError extends IOException {
        final String kind;
        final boolean retry;

        NetError(String kind, boolean retry, Exception cause) {
            super(kind + " (thử lại: " + (retry ? "có" : "không") + "): "
                    + (cause == null ? "" : cause.getMessage()), cause);
            this.kind = kind;
            this.retry = retry;
        }
    }

    /** Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định. */
    static Object[] parseEndpoint(String[] args, String defaultHost, int defaultPort) {
        String host = System.getenv().getOrDefault("APP_HOST", defaultHost);
        String port = System.getenv().getOrDefault("APP_PORT", String.valueOf(defaultPort));
        if (args.length >= 1) host = args[0];
        if (args.length >= 2) port = args[1];
        int p;
        try {
            p = Integer.parseInt(port);                                    // TO_INT
        } catch (NumberFormatException e) {
            throw new IllegalArgumentException("Cổng phải là số nguyên: " + port);   // ConfigError
        }
        if (p < 1 || p > 65535) throw new IllegalArgumentException("Cổng không hợp lệ: " + p);
        return new Object[] {host, p};
    }

    /** Mã giả 2.2 – SAFE_CONNECT: thử lần lượt các địa chỉ, có thời gian chờ kết nối, phân loại lỗi. */
    static Socket safeConnect(String host, int port, int timeoutMs) throws IOException {
        InetAddress[] addrs;
        try {
            addrs = InetAddress.getAllByName(host);                        // RESOLVE
        } catch (UnknownHostException e) {
            throw new NetError("NOT_FOUND", false, e);
        }
        int refused = 0;
        IOException last = null;
        for (InetAddress ip : addrs) {
            Socket s = new Socket();
            try {
                s.connect(new InetSocketAddress(ip, port), timeoutMs);    // CONNECT có CONNECT_TIMEOUT
                return s;
            } catch (ConnectException e) {                                 // nhận RST
                refused++;
                last = e;
                s.close();
            } catch (IOException e) {                                      // Timeout, Unreachable
                last = e;
                s.close();
            }
        }
        if (refused > 0 && refused == addrs.length) throw new NetError("REFUSED", false, last);
        throw new NetError("UNREACHABLE", true, last);
    }
}
