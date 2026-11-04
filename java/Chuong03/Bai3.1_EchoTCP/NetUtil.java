import java.io.IOException;
import java.io.OutputStream;

/** Tiện ích dùng chung của Bài 3.1. */
final class NetUtil {
    private NetUtil() {}

    /** Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_PORT > giá trị mặc định. */
    static Object[] parseEndpoint(String[] args, String defaultHost, int defaultPort) {
        String host = defaultHost;
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

    /** Mã giả 3.3 – SEND_ALL: OutputStream.write() của socket Java đã gửi hết mảng byte (lặp bên trong). */
    static void sendAll(OutputStream out, byte[] data, int count) throws IOException {
        out.write(data, 0, count);
        out.flush();
    }
}
