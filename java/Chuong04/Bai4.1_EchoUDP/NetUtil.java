/** Tiện ích dùng chung của Bài 4.1. */
final class NetUtil {
    private NetUtil() {}

    /** Mã giả 2.1 – PARSE_ENDPOINT: tham số dòng lệnh > biến môi trường APP_HOST/APP_PORT > giá trị mặc định. */
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
}
