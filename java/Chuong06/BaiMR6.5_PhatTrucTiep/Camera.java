import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.concurrent.TimeUnit;
import java.util.regex.Matcher;
import java.util.regex.Pattern;

/**
 * Bài mở rộng MR6.5 – Nguồn camera của máy phát: camera thật qua ffmpeg, mẫu thử testsrc hoặc camera giả lập
 * "bảng clapper". Chép từ CameraFfmpeg.java của MR6.3, sửa: layKhung(số khung, nhãn ms) và camera giả lập là
 * NguonClapper (đồng hồ + chớp trắng mỗi giây tròn).
 *
 * JDK không có API camera, nên chương trình chạy tiến trình `ffmpeg` (công cụ dòng lệnh, không phải thư viện) và
 * đọc khung hình thô từ ống (pipe) stdout của nó:
 *     ffmpeg -f <dshow|v4l2|avfoundation> -i <camera> -vf scale=160:120,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
 * Mỗi khung đúng 160 × 120 × 3 = 57 600 byte RGB, đưa thẳng vào đường gửi (mã hóa I/P bằng zlib, chia mảnh). Một luồng
 * nền đọc ống liên tục và chỉ giữ khung MỚI NHẤT – video trực tiếp không cần khung cũ.
 * Liệt kê camera: Windows `ffmpeg -list_devices true -f dshow -i dummy` (đọc stderr), Linux /dev/video*,
 * macOS `ffmpeg -f avfoundation -list_devices true -i ""`.
 * Không có ffmpeg -> chỉ còn camera giả lập; ffmpeg dừng/lỗi (không có camera, camera bận…) -> lõi máy phát tự chuyển
 * sang camera giả lập.
 */
final class Camera {
    private Camera() {}

    static final String GIA_LAP = "gia_lap", MAU_THU = "testsrc";          // hai "camera" không phải thiết bị thật
    static final String TEN_GIA_LAP = "Camera giả lập (bảng clapper)", TEN_MAU_THU = "Mẫu thử ffmpeg (testsrc)";
    static final int KICH_THUOC = Chung.RONG * Chung.CAO * 3;
    static final String OS = System.getProperty("os.name").toLowerCase(Locale.ROOT);

    /** Nguồn khung cho vòng gửi. */
    interface NguonCamera {
        byte[] layKhung(long so, long tsMs);       // RGB 160×120, hoặc null nếu chưa có khung
        void datFps(int fps);
        void dong();
        boolean that();
        String moTa();
        default String loi() { return ""; }        // khác "": nguồn đã dừng vì lỗi
    }

    /** Bọc NguonClapper theo cùng giao diện với QuaFfmpeg. */
    static final class GiaLap implements NguonCamera {
        final Chung.NguonClapper nguon;
        GiaLap(String ten, int ma, int fps) { nguon = new Chung.NguonClapper(ten, ma, fps); }
        public byte[] layKhung(long so, long tsMs) { return nguon.layKhung(so, tsMs); }
        public void datFps(int fps) { nguon.fps = fps; }
        public void dong() { }
        public boolean that() { return false; }
        public String moTa() { return "GIẢ LẬP – bảng clapper"; }
    }

    /** Chạy lệnh ngắn, trả về toàn bộ stderr + stdout (null nếu không chạy được, ví dụ chưa cài ffmpeg). */
    static String chayNgan(List<String> lenh) {
        try {
            Process p = new ProcessBuilder(lenh).redirectErrorStream(true).start();
            p.getOutputStream().close();
            byte[] ra = p.getInputStream().readAllBytes();
            p.waitFor(5, TimeUnit.SECONDS);
            return new String(ra, StandardCharsets.UTF_8);
        } catch (IOException | InterruptedException e) {
            return null;
        }
    }

    private static Boolean coFfmpeg;

    static synchronized boolean coFfmpeg() {
        if (coFfmpeg == null) {
            String ra = chayNgan(List.of("ffmpeg", "-hide_banner", "-version"));
            coFfmpeg = ra != null && ra.startsWith("ffmpeg version");
        }
        return coFfmpeg;
    }

    /** Danh sách {mã thiết bị, tên hiển thị} camera thật tìm được qua ffmpeg. */
    static List<String[]> lietKeCamera() {
        List<String[]> ds = new ArrayList<>();
        if (!coFfmpeg()) return ds;
        if (OS.contains("win")) {
            String ra = chayNgan(List.of("ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"));
            Matcher m = Pattern.compile("\"([^\"]+)\"\\s*\\(video\\)").matcher(ra == null ? "" : ra);
            while (m.find()) ds.add(new String[] {m.group(1), m.group(1)});
        } else if (OS.contains("mac")) {
            String ra = chayNgan(List.of("ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""));
            boolean video = false;
            for (String dong : (ra == null ? "" : ra).split("\n")) {
                if (dong.contains("video devices")) video = true;
                else if (dong.contains("audio devices")) video = false;
                else if (video) {
                    Matcher m = Pattern.compile("\\[(\\d+)\\]\\s+(.+)$").matcher(dong.strip());
                    if (m.find()) ds.add(new String[] {m.group(1), m.group(2).strip()});
                }
            }
        } else {
            File[] tep = new File("/dev").listFiles((d, n) -> n.matches("video\\d+"));
            if (tep != null) {
                Arrays.sort(tep);
                for (File f : tep) ds.add(new String[] {f.getPath(), f.getPath()});
            }
        }
        return ds;
    }

    /** Dòng lệnh ffmpeg thu camera `ma` (hoặc ảnh mẫu thử testsrc) ra khung RGB thô 160×120. */
    static List<String> lenhFfmpeg(String ma, int fps) {
        List<String> l = new ArrayList<>(List.of("ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin"));
        if (ma.equals(MAU_THU)) l.addAll(List.of("-re", "-f", "lavfi", "-i", "testsrc=size=" + Chung.RONG + "x" + Chung.CAO + ":rate=" + fps));
        else if (OS.contains("win")) l.addAll(List.of("-f", "dshow", "-i", "video=" + ma));   // ProcessBuilder tự đặt ngoặc kép
        else if (OS.contains("mac")) l.addAll(List.of("-f", "avfoundation", "-framerate", "30", "-i", ma));
        else l.addAll(List.of("-f", "v4l2", "-i", ma));
        l.addAll(List.of("-vf", "scale=" + Chung.RONG + ":" + Chung.CAO + ",fps=" + fps, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"));
        return l;
    }

    /** Camera qua tiến trình ffmpeg: luồng nền đọc từng khung 57 600 byte, giữ khung mới nhất; luồng khác đọc stderr. */
    static final class QuaFfmpeg implements NguonCamera {
        final boolean that;
        final String moTa;
        final Process p;
        volatile byte[] moiNhat;
        volatile String loi = "", dongLoiCuoi = "";
        volatile int soKhung;

        QuaFfmpeg(String ma, String ten, int fps) throws IOException {
            that = !ma.equals(MAU_THU);
            moTa = that ? "THẬT – " + ten + " (ffmpeg)" : "MẪU THỬ ffmpeg testsrc (không phải camera)";
            p = new ProcessBuilder(lenhFfmpeg(ma, fps)).start();
            p.getOutputStream().close();
            Chung.chayNen("ffmpeg-doc", this::doc);
            Chung.chayNen("ffmpeg-loi", () -> {
                try (BufferedReader r = new BufferedReader(new InputStreamReader(p.getErrorStream(), StandardCharsets.UTF_8))) {
                    String d;
                    while ((d = r.readLine()) != null) if (!d.isBlank()) dongLoiCuoi = d.strip();
                } catch (IOException ignored) { }
            });
        }

        private void doc() {
            try (InputStream ong = p.getInputStream()) {
                while (true) {
                    byte[] k = ong.readNBytes(KICH_THUOC);       // chặn tới khi đủ một khung hoặc hết
                    if (k.length < KICH_THUOC) break;            // ffmpeg kết thúc / lỗi mở camera
                    moiNhat = k;
                    soKhung++;
                }
            } catch (IOException ignored) { }
            try { p.waitFor(1, TimeUnit.SECONDS); } catch (InterruptedException ignored) { }
            if (loi.isEmpty()) loi = dongLoiCuoi.isEmpty() ? "ffmpeg đã dừng" : dongLoiCuoi;
        }

        public byte[] layKhung(long so, long tsMs) { return moiNhat; }   // khung không bị sửa sau khi đọc
        public void datFps(int fps) { }                                  // ffmpeg chạy theo fps lúc mở
        public void dong() { loi = "đã đóng"; p.destroy(); }
        public boolean that() { return that; }
        public String moTa() { return moTa; }
        public String loi() { return loi.equals("đã đóng") ? "" : loi; }
    }

    /** Mở camera đã chọn; mã GIA_LAP hoặc không có ffmpeg / không chạy được ffmpeg -> camera giả lập. */
    static NguonCamera mo(String maThietBi, String tenHienThi, String tenMayPhat, int maMayPhat, int fps) {
        if (!maThietBi.equals(GIA_LAP) && coFfmpeg()) {
            try {
                return new QuaFfmpeg(maThietBi, tenHienThi, fps);
            } catch (IOException ignored) { }
        }
        return new GiaLap(tenMayPhat, maMayPhat, fps);
    }
}
