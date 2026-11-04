import java.io.IOException;
import java.nio.file.Files;
import java.nio.file.Path;
import java.security.MessageDigest;
import java.security.NoSuchAlgorithmException;
import java.util.HashSet;
import java.util.HexFormat;
import java.util.Random;
import java.util.Set;

/** Bài 4.4 – Hằng số và tiện ích dùng chung cho bên gửi, bên nhận dừng và chờ (Mã giả 4.6, 4.7). */
final class SawCommon {
    private SawCommon() {}

    static final byte DATA = 0, ACK = 1, FIN = 2;                 // byte loại trong tiêu đề 5 byte
    static final int MAX_TRIES = 8, MAX_DGRAM = 1200, CHUNK = MAX_DGRAM - 5;   // khối ≤ 1195 byte
    static final int RTO_INIT = 200, RTO_MIN = 100;                            // ms, cho mạng nội bộ (Mục 4.4.2)

    /** Mã giả 4.6 dòng 1: MAX_RTO ← 60000 ms (trần của RTO, RFC 6298). Biến môi trường SAW_MAX_RTO (ms) ghi đè
     *  giá trị này CHỈ để chạy demo nhanh; không đặt biến thì bên nhận chờ 2 × 60000 ms = 120 s sau FIN. */
    private static final String MAX_RTO_ENV = System.getenv("SAW_MAX_RTO");
    static final int MAX_RTO = MAX_RTO_ENV == null || MAX_RTO_ENV.isEmpty() ? 60000 : Integer.parseInt(MAX_RTO_ENV);

    static String maxRtoNote() {
        return MAX_RTO_ENV == null || MAX_RTO_ENV.isEmpty() ? "MAX_RTO = " + MAX_RTO + " ms"
                : "MAX_RTO = " + MAX_RTO + " ms (ghi đè cho demo bằng biến môi trường SAW_MAX_RTO; mặc định 60000 ms)";
    }

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

    static String sha256File(String path) throws IOException {
        try {
            return HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(Path.of(path))));
        } catch (NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    /**
     * Mô phỏng mất gói ở chiều gửi đi (thêm cho bài thực hành, không có trong mã giả):
     * "" không mất; "0.1" mất ngẫu nhiên 10%; "3,7" mất datagram gửi đi thứ 3 và 7 (đếm từ 1, kể cả gửi lại).
     */
    static final class LossSim {
        final String spec;
        private final double rate;
        private final Set<Integer> picks = new HashSet<>();
        private final Random rnd = new Random();
        private int count;

        LossSim(String spec) {
            this.spec = spec == null ? "" : spec;
            if (this.spec.contains(".")) rate = Double.parseDouble(this.spec);
            else {
                rate = 0;
                for (String x : this.spec.split(",")) if (!x.isEmpty()) picks.add(Integer.parseInt(x));
            }
        }

        boolean drop() { return picks.contains(++count) | rnd.nextDouble() < rate; }
    }
}
