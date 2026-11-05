import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.nio.charset.StandardCharsets;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.TimeUnit;

/**
 * Bài mở rộng MR6.5 – Nguồn "tệp video" cho máy phát: phát lại hình + tiếng của một tệp (mp4, mkv, avi, webm…) như
 * một buổi phát trực tiếp (giống OBS "Media Source" hay `ffmpeg -re … rtmp://…`).
 *
 * Hai tiến trình ffmpeg cùng đọc tệp với `-re` (đọc theo tốc độ thật, không nhanh hơn thời gian phát) và
 * `-stream_loop -1` (hết tệp thì phát lại từ đầu):
 *     hình:  ffmpeg -re -stream_loop -1 -i <tệp> -an -vf scale=160:120:force_original_aspect_ratio=decrease,
 *                   pad=160:120:(ow-iw)/2:(oh-ih)/2,fps=<fps> -pix_fmt rgb24 -f rawvideo pipe:1
 *     tiếng: ffmpeg -re -stream_loop -1 -i <tệp> -vn -ac 1 -ar 16000 -f s16le pipe:1
 * Bộ lọc `fps=<fps>` cho đúng <fps> khung mỗi giây THỜI GIAN CỦA TỆP, nên khung thứ i có thời điểm trong tệp
 * i × 1000 / fps ms; tiếng có 16 000 mẫu mỗi giây của tệp.
 *
 * Đồng bộ tiếng-hình ở phía phát. Hai tiến trình khởi động lệch nhau vài chục – vài trăm ms; nếu cứ gửi "khung mới
 * nhất" thì hình và tiếng của TỆP lệch nhau ngay từ máy phát, người xem đồng bộ theo nhãn thời gian cũng không sửa
 * được. Cách làm ở đây (giống bộ ghép kênh – muxer – của bộ mã hóa thật):
 *   - Tiếng là đồng hồ chủ: vòng 20 ms của máy phát lấy lần lượt 320 mẫu của tệp. Chưa đủ mẫu (ffmpeg chưa chạy
 *     kịp) -> gửi im lặng và lechMs += 20; dồn quá 300 ms -> bỏ bớt 20 ms và lechMs −= 20. Vậy ở thời điểm phương
 *     tiện T máy phát đang gửi tiếng của giây T − lechMs trong tệp.
 *   - Hình: luồng đọc giữ ~3 s khung gần nhất kèm thời điểm trong tệp; khi máy phát chụp khung ở thời điểm phương
 *     tiện T, chọn khung có thời điểm trong tệp ≤ T − lechMs gần nhất. Nhờ vậy nhãn thời gian gửi đi khớp với nội
 *     dung của tệp, và cơ chế đồng bộ ở người xem (MR6.5) hoạt động như với camera.
 * Tệp không có tiếng (hoặc chọn micro khác) -> hình lấy khung mới nhất.
 */
final class TepVideo {
    static final String TEP = "tep";                                  // mã nguồn "tệp video" trong danh sách camera/micro
    static final String TEN_CAMERA = "Tệp video (chọn tệp bên dưới)", TEN_MIC = "Tiếng của tệp video";
    static final int BYTE_MOI_KHUNG_AM = Chung.MAU_MOI_KHUNG * 2;     // 320 mẫu × 2 byte (s16le)
    static final int DON_TOI_DA_MS = 300;                             // tiếng của tệp dồn quá mức này thì bỏ bớt
    static final int GIU_HINH_MS = 3000;                              // giữ khung hình của 3 s gần nhất

    static List<String> lenhHinh(String duongDan, int fps) {
        String loc = "scale=" + Chung.RONG + ":" + Chung.CAO + ":force_original_aspect_ratio=decrease,"
                + "pad=" + Chung.RONG + ":" + Chung.CAO + ":(ow-iw)/2:(oh-ih)/2,fps=" + fps;
        return List.of("ffmpeg", "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duongDan,
                "-an", "-vf", loc, "-pix_fmt", "rgb24", "-f", "rawvideo", "pipe:1");
    }

    static List<String> lenhTieng(String duongDan) {
        return List.of("ffmpeg", "-hide_banner", "-loglevel", "error", "-re", "-stream_loop", "-1", "-i", duongDan,
                "-vn", "-ac", "1", "-ar", String.valueOf(Chung.TAN_SO_MAU), "-f", "s16le", "pipe:1");
    }

    private record KhungTep(long ms, byte[] rgb) {}

    final String duongDan, ten;
    final int fps;
    private final Object khoa = new Object();
    private final ArrayDeque<KhungTep> hinh = new ArrayDeque<>();    // (ms trong tệp, khung rgb)
    private final ByteArrayOutputStream tieng = new ByteArrayOutputStream();
    private byte[] tiengDu = new byte[0];                             // phần tiếng chưa lấy (đọc dưới khóa)
    long soKhungDoc;
    int lechMs;                                                       // thời điểm phương tiện − thời điểm trong tệp (tiếng)
    volatile boolean dungTieng;                                       // micro của máy phát đang là tiếng của tệp này
    volatile String loiHinh = "", loiTieng = "";
    boolean hetTieng;
    private final Process ffmpegHinh, ffmpegTieng;

    TepVideo(String duongDan, int fps, boolean coTieng) throws IOException {
        if (!Camera.coFfmpeg()) throw new IOException("không có ffmpeg trong PATH");
        if (!new File(duongDan).isFile()) throw new IOException("không thấy tệp " + duongDan);
        this.duongDan = duongDan;
        this.fps = fps;
        this.ten = new File(duongDan).getName();
        hetTieng = !coTieng;
        ffmpegHinh = chay(lenhHinh(duongDan, fps));
        String[] loiH = {""};
        docLoi(ffmpegHinh, loiH);
        Chung.chayNen("tep-hinh", () -> {
            docHinh();
            loiHinh = loiH[0].isEmpty() ? "ffmpeg (hình) đã dừng" : loiH[0];
        });
        if (coTieng) {
            ffmpegTieng = chay(lenhTieng(duongDan));
            String[] loiT = {""};
            docLoi(ffmpegTieng, loiT);
            Chung.chayNen("tep-tieng", () -> {
                docTieng();
                synchronized (khoa) {
                    hetTieng = true;
                    loiTieng = loiT[0].isEmpty() ? "tệp không có tiếng" : loiT[0];
                }
            });
        } else {
            ffmpegTieng = null;
        }
    }

    private static Process chay(List<String> lenh) throws IOException {
        Process p = new ProcessBuilder(lenh).start();
        p.getOutputStream().close();                                  // stdin: không dùng
        return p;
    }

    /** Luồng đọc stderr (tránh đầy ống), giữ dòng cuối làm thông báo lỗi. */
    private static void docLoi(Process p, String[] dongCuoi) {
        Chung.chayNen("ffmpeg-loi", () -> {
            try (BufferedReader r = new BufferedReader(new InputStreamReader(p.getErrorStream(), StandardCharsets.UTF_8))) {
                String d;
                while ((d = r.readLine()) != null) if (!d.isBlank()) dongCuoi[0] = d.strip();
            } catch (IOException ignored) { }
        });
    }

    private void cho(Process p) {
        try { p.waitFor(1, TimeUnit.SECONDS); } catch (InterruptedException ignored) { }
    }

    private void docHinh() {
        try (InputStream ong = ffmpegHinh.getInputStream()) {
            while (true) {
                byte[] khung = ong.readNBytes(Camera.KICH_THUOC);
                if (khung.length < Camera.KICH_THUOC) break;
                synchronized (khoa) {
                    long ms = soKhungDoc * 1000 / fps;
                    soKhungDoc++;
                    hinh.addLast(new KhungTep(ms, khung));
                    while (!hinh.isEmpty() && hinh.peekFirst().ms() < ms - GIU_HINH_MS) hinh.pollFirst();
                }
            }
        } catch (IOException ignored) { }
        cho(ffmpegHinh);
    }

    private void docTieng() {
        byte[] b = new byte[BYTE_MOI_KHUNG_AM];
        try (InputStream ong = ffmpegTieng.getInputStream()) {
            int n;
            while ((n = ong.read(b)) > 0) {
                synchronized (khoa) { tieng.write(b, 0, n); }
            }
        } catch (IOException ignored) { }
        cho(ffmpegTieng);
    }

    // ------------------------------------------------------------ tiếng (vòng 20 ms của máy phát)
    /** 320 mẫu tiếp theo của tệp; chưa đủ -> im lặng (lechMs += 20). */
    short[] layTieng() {
        byte[] b;
        synchronized (khoa) {
            if (tieng.size() > 0) {                                   // gom phần mới đọc vào phần còn dư
                byte[] moi = tieng.toByteArray();
                tieng.reset();
                byte[] gop = new byte[tiengDu.length + moi.length];
                System.arraycopy(tiengDu, 0, gop, 0, tiengDu.length);
                System.arraycopy(moi, 0, gop, tiengDu.length, moi.length);
                tiengDu = gop;
            }
            int don = BYTE_MOI_KHUNG_AM * DON_TOI_DA_MS / Chung.KHUNG_MS, dau = 0;
            while (tiengDu.length - dau > don) {                      // dồn quá nhiều: bỏ 20 ms cũ nhất
                dau += BYTE_MOI_KHUNG_AM;
                lechMs -= Chung.KHUNG_MS;
            }
            if (tiengDu.length - dau < BYTE_MOI_KHUNG_AM) {
                if (dau > 0) tiengDu = java.util.Arrays.copyOfRange(tiengDu, dau, tiengDu.length);
                lechMs += Chung.KHUNG_MS;
                return new short[Chung.MAU_MOI_KHUNG];
            }
            b = java.util.Arrays.copyOfRange(tiengDu, dau, dau + BYTE_MOI_KHUNG_AM);
            tiengDu = java.util.Arrays.copyOfRange(tiengDu, dau + BYTE_MOI_KHUNG_AM, tiengDu.length);
        }
        short[] m = new short[Chung.MAU_MOI_KHUNG];                   // s16le: byte thấp trước
        for (int i = 0; i < m.length; i++) m[i] = (short) ((b[2 * i] & 0xFF) | (b[2 * i + 1] << 8));
        return m;
    }

    // ------------------------------------------------------------ hình (vòng fps của máy phát)
    byte[] layHinh(long tsMs) {
        synchronized (khoa) {
            if (hinh.isEmpty()) return null;
            if (!dungTieng || hetTieng) return hinh.peekLast().rgb();   // không có tiếng của tệp: khung mới nhất
            long can = tsMs - lechMs;                                    // thời điểm trong tệp ứng với tiếng đang gửi
            byte[] chon = null;
            for (KhungTep k : hinh) {
                if (k.ms() <= can) chon = k.rgb();
                else break;
            }
            return chon != null ? chon : hinh.peekFirst().rgb();      // hình chưa tới kịp: khung sớm nhất đang có
        }
    }

    int lechMs() { synchronized (khoa) { return lechMs; } }

    void dong() {
        for (Process p : new Process[] {ffmpegHinh, ffmpegTieng}) {
            if (p == null) continue;
            p.destroyForcibly();
            cho(p);
        }
    }

    /** Phần hình của TepVideo theo giao diện camera của máy phát. */
    static final class CameraTep implements Camera.NguonCamera {
        final TepVideo tep;
        CameraTep(TepVideo tep) { this.tep = tep; }
        public byte[] layKhung(long so, long tsMs) { return tep.layHinh(tsMs); }   // khung không bị sửa sau khi đọc
        public void datFps(int fps) { }                                         // ffmpeg chạy theo fps lúc mở
        public void dong() { tep.dong(); }
        public boolean that() { return false; }
        public String moTa() { return "TỆP VIDEO – " + tep.ten; }
        public String loi() { return tep.loiHinh; }
    }

    /** Phần tiếng của TepVideo theo giao diện micro của máy phát. */
    static final class MicTep implements ThietBiAmThanh.NguonAm {
        final TepVideo tep;
        MicTep(TepVideo tep) { this.tep = tep; tep.dungTieng = true; }
        public short[] layKhung() { return tep.layTieng(); }
        public void dong() { tep.dong(); }
        public boolean that() { return false; }
        public String moTa() { return "TIẾNG CỦA TỆP – " + tep.ten; }
    }

    record MoTep(CameraTep camera, MicTep mic) {}

    /** Mở tệp video; trả về (CameraTep, MicTep hoặc null). Lỗi -> IOException. */
    static MoTep moTep(String duongDan, int fps, boolean layTieng) throws IOException {
        TepVideo tep = new TepVideo(duongDan, fps, layTieng);
        return new MoTep(new CameraTep(tep), layTieng ? new MicTep(tep) : null);
    }

    /**
     * Tạo tệp mp4 mẫu bằng ffmpeg (để thử khi chưa có tệp): ảnh testsrc2 chớp trắng 0,1 s đầu mỗi giây, tiếng 440 Hz
     * nhỏ + bíp 1000 Hz đúng lúc chớp – nhìn/nghe là thấy hình và tiếng có khớp.
     */
    static String taoTepMau(String duongDan, int giay) throws IOException {
        String hinh = "testsrc2=size=320x240:rate=25:duration=" + giay + ","
                + "drawbox=x=0:y=0:w=iw:h=ih:color=white@0.85:t=fill:enable='lt(mod(t\\,1)\\,0.1)'";
        String tieng = "aevalsrc='0.08*sin(2*PI*440*t)+0.4*sin(2*PI*1000*t)*lt(mod(t\\,1)\\,0.1)':"
                + "s=" + Chung.TAN_SO_MAU + ":d=" + giay;
        List<String> lenh = new ArrayList<>(List.of("ffmpeg", "-hide_banner", "-loglevel", "error", "-y", "-f", "lavfi",
                "-i", hinh, "-f", "lavfi", "-i", tieng, "-c:v", "libx264", "-pix_fmt", "yuv420p", "-g", "25",
                "-c:a", "aac", "-shortest", duongDan));
        String[] r = chayCho(lenh);
        if (!r[0].equals("0")) {                                      // ffmpeg thiếu libx264: dùng mpeg4
            lenh.set(lenh.indexOf("libx264"), "mpeg4");
            r = chayCho(lenh);
        }
        if (!r[0].equals("0")) {
            String[] dong = r[1].strip().split("\n");
            throw new IOException(r[1].isBlank() ? "ffmpeg lỗi" : dong[dong.length - 1]);
        }
        return duongDan;
    }

    /** Chạy lệnh tới khi xong; trả về {mã thoát, stderr}. */
    private static String[] chayCho(List<String> lenh) throws IOException {
        Process p = new ProcessBuilder(lenh).redirectOutput(ProcessBuilder.Redirect.DISCARD).start();
        p.getOutputStream().close();
        String err = new String(p.getErrorStream().readAllBytes(), StandardCharsets.UTF_8);
        try {
            return new String[] {String.valueOf(p.waitFor()), err};
        } catch (InterruptedException e) {
            p.destroyForcibly();
            throw new IOException("bị ngắt");
        }
    }
}
