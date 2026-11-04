/**
 * Bài 2.1 – Đọc địa chỉ và cổng từ tham số dòng lệnh, biến môi trường
 * (Mã giả 2.1 PARSE_ENDPOINT, Mục 2.1.3 Giáo trình Lập trình mạng). Chương trình chỉ in cấu hình, không mở socket.
 * Biên dịch: javac -encoding UTF-8 -d out *.java
 * Chạy:      java -cp out DocCauHinh [host] [port]          (mặc định 0.0.0.0 9000)
 * Ví dụ:     APP_PORT=9100 java -cp out DocCauHinh          (Linux/macOS)
 *            set APP_PORT=9100 && java -cp out DocCauHinh   (Windows cmd)
 */
public class DocCauHinh {

    /** Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST, APP_PORT > mặc định. */
    static Object[] parseEndpoint(String[] args, String defaultHost, int defaultPort) throws ConfigError {
        String host = defaultHost;                                       // dòng 2
        String port = String.valueOf(defaultPort);                       // dòng 3
        if (System.getenv("APP_HOST") != null) host = System.getenv("APP_HOST"); // dòng 4
        if (System.getenv("APP_PORT") != null) port = System.getenv("APP_PORT"); // dòng 5
        if (args.length >= 1) host = args[0];                            // dòng 6: tham số dòng lệnh ghi đè tất cả
        if (args.length >= 2) port = args[1];                            // dòng 7
        int p;
        try {                                                            // dòng 8–12: TO_INT
            p = Integer.parseInt(port);                                  // "abc" → NumberFormatException
        } catch (NumberFormatException e) {
            throw new ConfigError("Cổng phải là số nguyên: " + port);
        }
        if (p < 1 || p > 65535)                                          // dòng 13–15: cổng 16 bit, khác 0
            throw new ConfigError("Cổng không hợp lệ: " + p);
        return new Object[] {host, p};                                   // dòng 16
    }

    public static void main(String[] args) {
        String envHost = System.getenv().getOrDefault("APP_HOST", "(không đặt)");
        String envPort = System.getenv().getOrDefault("APP_PORT", "(không đặt)");
        String argText = args.length > 0 ? String.join(" ", args) : "(không có)";
        // In dữ liệu vào để thấy rõ giá trị nào thắng
        System.out.println("[cấu hình] APP_HOST=" + envHost + ", APP_PORT=" + envPort + ", tham số: " + argText);
        try {
            Object[] ep = parseEndpoint(args, "0.0.0.0", 9000);
            System.out.println("[cấu hình] Kết quả: host=" + ep[0] + ", port=" + ep[1]);
        } catch (ConfigError e) {
            System.out.println("[cấu hình] ConfigError: " + e.getMessage());
            System.exit(2);
        }
    }
}
