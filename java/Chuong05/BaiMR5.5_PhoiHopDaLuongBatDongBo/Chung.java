import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.PrintStream;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.HexFormat;
import java.util.Locale;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR5.5 – Phần dùng chung: hằng số, việc CPU (băm), hàm "thư viện cũ" kiểu chặn, nhật ký.
 * Giao thức (mỗi kết nối một yêu cầu) giống bản Python/Go:
 *   BAM mã chế_độ số_byte số_vòng hạn_ms \n + dữ liệu    chế_độ: chan | tach | gioihan
 *       → "OK mã 16-hex-đầu-SHA-256" hoặc "HETGIO mã"
 *   CAMBIEN mã     → "CAMBIEN mã giá-trị tên-luồng-đã-hoàn-tất-CompletableFuture"
 *   DATLAI         → "OK";   THONGKE nhãn → "TK trễ-nhịp-ms số-mẫu trễ-giao-mẫu-ms nhóm-tính-tối-đa"
 */
final class Chung {
    static final int N_YC = 16, SO_BYTE = 64 * 1024, SO_VONG = 400, MS_CU = 20, NHOM_TINH = 2, GIOI_HAN = 4,
            NHIP_MS = 10, CAM_BIEN_MS = 50;
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
    static final long T0 = System.nanoTime();

    static double bayGioMs() { return (System.nanoTime() - T0) / 1e6; }

    static String f(String fmt, Object... a) { return String.format(Locale.ROOT, fmt, a); }

    /** Dữ liệu thử: byte thứ i = i mod 251 (giống mọi bản ngôn ngữ). */
    static byte[] duLieu(int n) {
        byte[] b = new byte[n];
        for (int i = 0; i < n; i++) b[i] = (byte) (i % 251);
        return b;
    }

    /** Tên dễ đọc của luồng hiện tại; luồng ảo không có tên → "ảo#mã". */
    static String tenLuong() {
        Thread t = Thread.currentThread();
        return t.isVirtual() ? "ảo#" + t.threadId() : t.getName();
    }

    record KetQuaBam(String hex, String ten, int dungO) { }

    /** Việc nặng CPU: SHA-256 của dữ liệu lặp soVong lần; kiểm tra cờ hủy trước mỗi vòng. */
    static KetQuaBam bam(byte[] data, int soVong, AtomicBoolean huy) {
        try {
            MessageDigest md = MessageDigest.getInstance("SHA-256");
            for (int r = 0; r < soVong; r++) {
                if (huy != null && huy.get()) return new KetQuaBam(null, tenLuong(), r);
                md.update(data);
            }
            return new KetQuaBam(HexFormat.of().formatHex(md.digest()).substring(0, 16), tenLuong(), -1);
        } catch (Exception e) {
            throw new IllegalStateException(e);
        }
    }

    /** Giả lập hàm của thư viện CŨ chỉ có API chặn (JDBC, socket chặn...): giữ luồng MS_CU ms. */
    static String traCuuCu() {
        try { Thread.sleep(MS_CU); } catch (InterruptedException e) { Thread.currentThread().interrupt(); }
        return tenLuong();
    }

    static synchronized void log(String msg) {
        OUT.println(f("[%7.3f s] %-16s| %s", bayGioMs() / 1000, tenLuong(), msg));
    }

    static synchronized void say(String msg) { OUT.println(msg); }

    /** Máy tạo tải: gửi một dòng (kèm dữ liệu) rồi đọc một dòng trả lời – socket chặn trên luồng ảo. */
    static String gui(String host, int port, String line, byte[] data) {
        try (Socket s = new Socket(host, port)) {
            OutputStream o = s.getOutputStream();
            o.write((line + "\n").getBytes(StandardCharsets.UTF_8));
            if (data != null) o.write(data);
            o.flush();
            return new BufferedReader(new InputStreamReader(s.getInputStream(), StandardCharsets.UTF_8)).readLine();
        } catch (Exception e) {
            return "LOI " + e.getMessage();
        }
    }
}
