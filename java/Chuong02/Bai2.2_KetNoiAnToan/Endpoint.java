/** Mã giả 2.1 (rút gọn) – PARSE_ENDPOINT: tham số dòng lệnh > APP_HOST/APP_PORT > giá trị mặc định. */
final class Endpoint {
    private Endpoint() {}

    static Object[] parse(String[] args, String defaultHost, int defaultPort) throws ConfigError {
        String host = System.getenv().getOrDefault("APP_HOST", defaultHost);
        String port = System.getenv().getOrDefault("APP_PORT", String.valueOf(defaultPort));
        if (args.length >= 1) host = args[0];
        if (args.length >= 2) port = args[1];
        int p;
        try {
            p = Integer.parseInt(port);
        } catch (NumberFormatException e) {
            throw new ConfigError("Cổng phải là số nguyên: " + port);
        }
        if (p < 1 || p > 65535) throw new ConfigError("Cổng không hợp lệ: " + p);
        return new Object[] {host, p};
    }
}
