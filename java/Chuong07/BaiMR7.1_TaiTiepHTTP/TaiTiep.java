import java.io.File;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URI;
import java.net.URL;
import java.nio.file.Files;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Locale;

/**
 * Bài mở rộng MR7.1 – Client tải tệp HTTP có tải tiếp (resume) khi kết nối bị đứt.
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 7.1 (client HTTP), Bài 5.6 (thử lại có lùi thời gian)
 * và Bài 3.4 (kiểm tra SHA-256). Quy trình:
 *  1. HEAD lấy kích thước, ETag và mã băm SHA-256 của tệp.
 *  2. GET với "Range: bytes=<đã_có>-" và "If-Range: <ETag>"; ghi nối vào tệp cục bộ.
 *     206 -> nối tiếp; 200 -> server gửi lại từ đầu (tệp đã đổi hoặc không hỗ trợ Range).
 *  3. Nếu nhận ít hơn Content-Length (kết nối đứt) -> chờ lùi thời gian rồi thử lại từ chỗ dừng.
 *  4. Kiểm tra kích thước và SHA-256.
 * Khác http.client của Python, HttpURLConnection BÁO LỖI ("Premature EOF") khi kết nối đóng trước khi đủ
 * Content-Length; chương trình bắt lỗi đó và vẫn giữ phần đã ghi.
 *
 * Chạy: java -cp out TaiTiep <url> <tệp_lưu> [--so-lan-thu N]
 */
public class TaiTiep {
    record Head(long size, String etag, String sha) {}

    static Head head(URL u) throws IOException {
        HttpURLConnection c = (HttpURLConnection) u.openConnection();
        c.setRequestMethod("HEAD");
        c.setConnectTimeout(5000);
        c.setReadTimeout(5000);
        try {
            if (c.getResponseCode() != 200) throw new IOException("HEAD trả " + c.getResponseCode());
            return new Head(c.getContentLengthLong(), c.getHeaderField("ETag"), c.getHeaderField("X-Checksum-SHA256"));
        } finally {
            c.disconnect();
        }
    }

    /** Một lần GET; trả về số byte có trong tệp cục bộ sau lần này. */
    static long getRange(URL u, File path, long have, String etag) throws IOException {
        HttpURLConnection c = (HttpURLConnection) u.openConnection();
        c.setConnectTimeout(5000);
        c.setReadTimeout(5000);
        if (have > 0) {
            c.setRequestProperty("Range", "bytes=" + have + "-");
            c.setRequestProperty("If-Range", etag);
        }
        try {
            int status = c.getResponseCode();
            System.out.println("[tai] -> " + status + " " + c.getResponseMessage()
                    + (status == 206 ? ", " + c.getHeaderField("Content-Range") : ""));
            if (status == 416) return have;                       // dải không thỏa mãn được: giữ nguyên phần đã có
            boolean append;
            if (status == 200) {
                append = false;                                   // server gửi toàn bộ: ghi đè từ đầu
                have = 0;
            } else if (status == 206) {
                append = true;
            } else {
                throw new IOException("Mã trạng thái không mong đợi " + status);
            }
            long expected = c.getContentLengthLong(), got = 0;
            try (InputStream in = c.getInputStream(); FileOutputStream f = new FileOutputStream(path, append)) {
                byte[] buf = new byte[16384];
                while (got < expected) {
                    int n;
                    try {
                        n = in.read(buf, 0, (int) Math.min(buf.length, expected - got));
                    } catch (IOException e) {                     // kết nối đóng sớm: HttpURLConnection báo lỗi
                        n = -1;
                    }
                    if (n == -1) break;
                    f.write(buf, 0, n);
                    got += n;
                }
            }
            if (got < expected) System.out.println("[tai] Kết nối đứt: nhận " + got + "/" + expected + " byte của lần này");
            return have + got;
        } finally {
            c.disconnect();
        }
    }

    public static void main(String[] args) throws Exception {
        String url = args[0];
        File out = new File(args[1]);
        int soLanThu = 5;
        for (int i = 2; i + 1 < args.length; i += 2) if (args[i].equals("--so-lan-thu")) soLanThu = Integer.parseInt(args[i + 1]);
        URL u = URI.create(url).toURL();
        Head h = head(u);
        System.out.println("[tai] HEAD: " + h.size() + " byte, ETag " + h.etag());
        long have = out.exists() ? out.length() : 0;
        for (int attempt = 1; attempt <= soLanThu; attempt++) {
            if (have >= h.size()) break;
            System.out.println("[tai] Lần " + attempt + ": đã có " + have + " byte, xin " + (have > 0 ? "Range: bytes=" + have + "-" : "toàn bộ"));
            try {
                have = getRange(u, out, have, h.etag());
            } catch (IOException e) {
                System.out.println("[tai] Lỗi: " + e.getMessage());
                have = out.exists() ? out.length() : 0;
            }
            if (have < h.size()) {
                double delay = 0.2 * Math.pow(2, attempt - 1);      // lùi thời gian theo cấp số nhân (Bài 5.6)
                System.out.println(String.format(Locale.ROOT, "[tai] Chờ %.1f s rồi tải tiếp", delay));
                Thread.sleep((long) (delay * 1000));
            }
        }
        if (have < h.size()) {
            System.out.println("[tai] THẤT BẠI sau " + soLanThu + " lần thử: có " + have + "/" + h.size() + " byte");
            System.exit(1);
        }
        String digest = HexFormat.of().formatHex(MessageDigest.getInstance("SHA-256").digest(Files.readAllBytes(out.toPath())));
        System.out.println("[tai] Đủ " + have + "/" + h.size() + " byte; SHA-256 " + (digest.equals(h.sha()) ? "KHỚP" : "SAI") + " (" + digest.substring(0, 16) + "...)");
    }
}
