import java.io.EOFException;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.io.RandomAccessFile;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Locale;
import java.util.Map;

/**
 * Bài mở rộng MR5.1 – Khung giao thức dùng chung: định khung JSON, thùng token, băm SHA-256.
 *
 * Giao thức (một kết nối TCP cho mỗi lượt truyền):
 *  - Một khung điều khiển JSON có tiền tố độ dài 4 byte (big-endian) mở đầu.
 *  - Sau đó là luồng byte nhị phân của tệp (từ offset tới hết) để hỗ trợ tải tiếp (resume).
 * Chép gọn ý tưởng định khung từ Mã giả 5.1 (Bài 5.1) và băm SHA-256 từ Bài 3.9/MR7.1.
 */
final class Khung {
    private Khung() {}

    static final int CHUNK = 32 * 1024;                 // cỡ mỗi lần đọc/ghi
    static final double KHONG_GIOI_HAN = 1L << 40;      // "không giới hạn" tốc độ (1 TiB/s) – dùng cho mặc định

    /** Gửi một khung điều khiển JSON: 4 byte độ dài + nội dung UTF-8. */
    static void guiKhung(Socket s, Map<String, Object> obj) throws IOException {
        byte[] data = Json.stringify(obj).getBytes(StandardCharsets.UTF_8);
        OutputStream out = s.getOutputStream();
        out.write(ByteBuffer.allocate(4 + data.length).putInt(data.length).put(data).array());
    }

    /** Đọc đúng n byte (hoặc báo lỗi nếu kết nối đóng giữa chừng). */
    static byte[] docDu(InputStream in, int n) throws IOException {
        byte[] b = in.readNBytes(n);
        if (b.length < n) throw new EOFException("Kết nối đóng khi đang đọc");
        return b;
    }

    /** Nhận một khung điều khiển JSON. */
    @SuppressWarnings("unchecked")
    static Map<String, Object> nhanKhung(InputStream in) throws IOException {
        int n = ByteBuffer.wrap(docDu(in, 4)).getInt();
        return (Map<String, Object>) Json.parse(new String(docDu(in, n), StandardCharsets.UTF_8));
    }

    /** Băm SHA-256 của tệp, trả chuỗi hex. */
    static String sha256Tep(String path) throws IOException {
        try (RandomAccessFile f = new RandomAccessFile(path, "r")) {
            MessageDigest h = MessageDigest.getInstance("SHA-256");
            byte[] buf = new byte[CHUNK];
            int n;
            while ((n = f.read(buf)) > 0) h.update(buf, 0, n);
            return HexFormat.of().formatHex(h.digest());
        } catch (java.security.NoSuchAlgorithmException e) {
            throw new IllegalStateException(e);
        }
    }

    /**
     * Thùng token giới hạn tốc độ (byte/giây), đổ đầy liên tục; có thể đổi tốc độ lúc chạy.
     * Dùng chung cho mức phiên (session) và mức từng tệp. tieuThu(n) chặn tới khi đủ n token.
     */
    static final class ThungToken {
        volatile double rate;
        private double sucChua, token;
        private long moc = System.nanoTime();

        ThungToken(double rate) {
            this.rate = rate;
            this.sucChua = rate;                              // cho phép bùng một giây
            this.token = Math.min(rate, CHUNK * 4);           // bùng khởi động nhỏ -> tốc độ đo sát giới hạn
        }

        private void doDay() {
            long now = System.nanoTime();
            token = Math.min(sucChua, token + (now - moc) / 1e9 * rate);
            moc = now;
        }

        synchronized void datRate(double r) {
            doDay();
            rate = r;
            sucChua = r;                                      // sức chứa = 1 giây ở tốc độ mới
            // Giới hạn "bùng" còn 0,25 giây để nhiều tệp chia sẻ đều hơn, tốc độ đo sát giới hạn phiên.
            token = Math.min(token, r * 0.25);
        }

        interface DungKhi { boolean test(); }

        /** Chặn tới khi lấy được n token. dungKhi() trả true thì bỏ dở (khi pause/stop). */
        boolean tieuThu(int n, DungKhi dungKhi) throws InterruptedException {
            while (!dungKhi.test()) {
                double thieu;
                synchronized (this) {
                    doDay();
                    if (token >= n) {
                        token -= n;
                        return true;
                    }
                    thieu = (n - token) / rate;
                }
                Thread.sleep((long) Math.max(1, Math.min(thieu, 0.05) * 1000));
            }
            return false;
        }
    }

    /** Phân tích chuỗi tốc độ: '512K', '1M', '2MiB', '1048576' -> số byte/giây. */
    static double docSoByte(String s) {
        s = s.strip().toUpperCase(Locale.ROOT).replace("IB", "").replace("B", "");
        long nhan = 1;
        if (s.endsWith("K")) { nhan = 1024; s = s.substring(0, s.length() - 1); }
        else if (s.endsWith("M")) { nhan = 1024 * 1024; s = s.substring(0, s.length() - 1); }
        else if (s.endsWith("G")) { nhan = 1024L * 1024 * 1024; s = s.substring(0, s.length() - 1); }
        return (long) (Double.parseDouble(s) * nhan);
    }

    /** Số byte -> chuỗi dễ đọc (KiB/MiB). */
    static String dinhDangByte(long n) {
        if (n >= 1024 * 1024) return String.format(Locale.ROOT, "%.2f MiB", n / (1024.0 * 1024));
        if (n >= 1024) return String.format(Locale.ROOT, "%.2f KiB", n / 1024.0);
        return n + " B";
    }
}
