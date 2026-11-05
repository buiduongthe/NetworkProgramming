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
 * Camera cho bản giao diện. JDK không có API camera, nên camera thật được đọc qua tiến trình ffmpeg (không thư viện
 * ngoài): ffmpeg thu từ thiết bị, thu nhỏ về 160×120, đổi sang RGB 24 bit và ghi các khung thô liên tiếp ra stdout
 * (pipe:1); chương trình đọc đúng 160 × 120 × 3 = 57 600 byte cho mỗi khung rồi đưa vào đường gửi sẵn có (thu nhỏ
 * theo yêu cầu server, nén zlib, chia mảnh). Không có ffmpeg hoặc camera -> "Camera giả lập" (CameraTongHop).
 *
 *  - Liệt kê: Windows  ffmpeg -hide_banner -list_devices true -f dshow -i dummy   (đọc stderr, dòng "Tên" (video))
 *             Linux    các tệp /dev/video*
 *             macOS    ffmpeg -hide_banner -f avfoundation -list_devices true -i ""   (dòng [n] Tên)
 *  - Thu:     ffmpeg -f dshow -i video=Tên | -f v4l2 -i /dev/video0 | -f avfoundation -i n
 *             -vf scale=160:120,fps=15 -pix_fmt rgb24 -f rawvideo pipe:1
 * Mục "Mẫu thử ffmpeg (testsrc)" dùng nguồn ảo lavfi của ffmpeg – kiểm tra được đường ffmpeg trên máy không có camera.
 */
final class CameraFfmpeg {
    private CameraFfmpeg() {}

    static final String GIA_LAP = "Camera giả lập", MAU_THU = "Mẫu thử ffmpeg (testsrc)";
    static final int FPS_THU = 15;                                  // ffmpeg xuất 15 khung/s; vòng gửi tự giới hạn
    static final String OS = System.getProperty("os.name").toLowerCase(Locale.ROOT);

    /** Nguồn khung cho vòng gửi: RGB 160×120 mới nhất, hoặc null nếu chưa có. */
    interface NguonCamera {
        byte[] layKhung(int so);
        void dong();
        boolean that();
        String moTa();
        default String loi() { return null; }                     // khác null: nguồn đã dừng vì lỗi
    }

    /** Camera tổng hợp của bản dòng lệnh (nền sóng màu, tên, số khung). */
    static final class GiaLap implements NguonCamera {
        final Video.CameraTongHop cam;
        GiaLap(String ten, int ma) { cam = new Video.CameraTongHop(ten, ma); }
        public byte[] layKhung(int so) { return cam.layKhung(so); }
        public void dong() { }
        public boolean that() { return false; }
        public String moTa() { return GIA_LAP; }
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

    static Boolean coFfmpeg;

    static synchronized boolean coFfmpeg() {
        if (coFfmpeg == null) {
            String ra = chayNgan(List.of("ffmpeg", "-hide_banner", "-version"));
            coFfmpeg = ra != null && ra.startsWith("ffmpeg version");
        }
        return coFfmpeg;
    }

    /** Danh sách cho ô "Chọn camera": thiết bị thật (nếu có ffmpeg), mẫu thử ffmpeg, rồi "Camera giả lập". */
    static List<String> dsCamera() {
        List<String> ds = new ArrayList<>();
        if (coFfmpeg()) {
            if (OS.contains("win")) {
                String ra = chayNgan(List.of("ffmpeg", "-hide_banner", "-list_devices", "true", "-f", "dshow", "-i", "dummy"));
                Matcher m = Pattern.compile("\"([^\"]+)\"\\s*\\(video\\)").matcher(ra == null ? "" : ra);
                while (m.find()) ds.add(m.group(1));
            } else if (OS.contains("mac")) {
                String ra = chayNgan(List.of("ffmpeg", "-hide_banner", "-f", "avfoundation", "-list_devices", "true", "-i", ""));
                boolean video = false;
                for (String dong : (ra == null ? "" : ra).split("\n")) {
                    if (dong.contains("AVFoundation video devices")) video = true;
                    else if (dong.contains("AVFoundation audio devices")) video = false;
                    else if (video) {
                        Matcher m = Pattern.compile("\\[(\\d+)\\] (.+)$").matcher(dong.strip());
                        if (m.find()) ds.add(m.group(1) + ": " + m.group(2));
                    }
                }
            } else {
                File[] tep = new File("/dev").listFiles((d, n) -> n.matches("video\\d+"));
                if (tep != null) {
                    Arrays.sort(tep);
                    for (File f : tep) ds.add(f.getPath());
                }
            }
            ds.add(MAU_THU);
        }
        ds.add(GIA_LAP);
        return ds;
    }

    /** Lệnh ffmpeg thu từ thiết bị đã chọn, xuất RGB thô 160×120. */
    static List<String> lenhThu(String thietBi) {
        List<String> l = new ArrayList<>(List.of("ffmpeg", "-hide_banner", "-loglevel", "error", "-nostdin"));
        if (thietBi.equals(MAU_THU)) l.addAll(List.of("-re", "-f", "lavfi", "-i", "testsrc=size=160x120:rate=" + FPS_THU));
        else if (OS.contains("win")) l.addAll(List.of("-f", "dshow", "-i", "video=" + thietBi));   // ProcessBuilder tự đặt ngoặc kép
        else if (OS.contains("mac")) l.addAll(List.of("-f", "avfoundation", "-framerate", "30", "-i", thietBi.split(":", 2)[0]));
        else l.addAll(List.of("-f", "v4l2", "-i", thietBi));
        l.addAll(List.of("-vf", "scale=" + Video.RONG + ":" + Video.CAO + ",fps=" + FPS_THU, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1"));
        return l;
    }

    /** Camera thật (hoặc mẫu thử) qua ffmpeg: một luồng đọc khung từ stdout, một luồng đọc thông báo lỗi. */
    static final class QuaFfmpeg implements NguonCamera {
        final String thietBi;
        final Process p;
        volatile byte[] moiNhat;
        volatile String loi, dongLoiCuoi = "";
        volatile int soKhungDoc;

        QuaFfmpeg(String thietBi) throws IOException {
            this.thietBi = thietBi;
            p = new ProcessBuilder(lenhThu(thietBi)).start();
            Thread.ofPlatform().daemon().name("ffmpeg-doc").start(this::docKhung);
            Thread.ofPlatform().daemon().name("ffmpeg-loi").start(() -> {
                try (BufferedReader r = new BufferedReader(new InputStreamReader(p.getErrorStream(), StandardCharsets.UTF_8))) {
                    String d;
                    while ((d = r.readLine()) != null) if (!d.isBlank()) dongLoiCuoi = d.strip();
                } catch (IOException ignored) { }
            });
        }

        private void docKhung() {
            int co = Video.RONG * Video.CAO * 3;
            try (InputStream in = p.getInputStream()) {
                while (true) {
                    byte[] k = in.readNBytes(co);                  // chặn tới khi đủ một khung
                    if (k.length < co) break;                      // ffmpeg kết thúc
                    moiNhat = k;
                    soKhungDoc++;
                }
            } catch (IOException ignored) { }
            try { p.waitFor(1, TimeUnit.SECONDS); } catch (InterruptedException ignored) { }
            loi = "ffmpeg dừng" + (dongLoiCuoi.isEmpty() ? "" : ": " + dongLoiCuoi);
        }

        public byte[] layKhung(int so) { return moiNhat; }
        public void dong() { loi = "đã đóng"; p.destroy(); }
        public boolean that() { return !thietBi.equals(MAU_THU); }
        public String moTa() { return thietBi.equals(MAU_THU) ? "mẫu thử ffmpeg" : "camera thật: " + thietBi; }
        public String loi() { return loi; }
    }

    /** Mở nguồn theo lựa chọn; lỗi (không có ffmpeg…) -> camera giả lập, lý do gửi cho baoLoi. */
    static NguonCamera mo(String luaChon, String ten, int ma, java.util.function.Consumer<String> baoLoi) {
        if (luaChon != null && !luaChon.equals(GIA_LAP)) {
            try {
                return new QuaFfmpeg(luaChon);
            } catch (IOException e) {
                baoLoi.accept("Không chạy được ffmpeg (" + e.getMessage() + ") – dùng camera giả lập");
            }
        }
        return new GiaLap(ten, ma);
    }
}
