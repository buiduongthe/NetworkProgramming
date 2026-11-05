import java.util.ArrayList;
import java.util.List;
import java.util.concurrent.ArrayBlockingQueue;
import javax.sound.sampled.AudioFormat;
import javax.sound.sampled.AudioSystem;
import javax.sound.sampled.DataLine;
import javax.sound.sampled.LineUnavailableException;
import javax.sound.sampled.Mixer;
import javax.sound.sampled.SourceDataLine;
import javax.sound.sampled.TargetDataLine;

/**
 * Thiết bị âm thanh cho bản giao diện – chỉ dùng javax.sound.sampled của JDK.
 *
 *  - Micro thật: TargetDataLine 16 kHz, 16 bit, mono. Một luồng nền đọc liên tục từng khung 640 byte (20 ms) và
 *    đặt vào hàng đợi nhỏ; nhịp 20 ms của chương trình chỉ lấy ra (không bao giờ chờ thiết bị).
 *  - Loa thật: SourceDataLine; chỉ ghi khi bộ đệm của loa còn chỗ, đầy thì bỏ khung (không chặn nhịp 20 ms).
 *  - Không có thiết bị (máy chủ không có card âm thanh, container…) hoặc người dùng chọn "Giả lập": nguồn sin.
 * Thiết bị thật chỉ được mở khi người dùng bấm Start Mic / Start Audio / Nghe – không mở lúc khởi động.
 */
final class ThietBiAmThanh {
    private ThietBiAmThanh() {}

    /** Nguồn âm cho nhịp 20 ms: mỗi lần gọi trả một khung 320 mẫu, hoặc null nếu chưa có dữ liệu. */
    interface NguonKhung {
        short[] layKhung();
        void dong();
        boolean that();                                   // true: thiết bị thật; false: giả lập
        String moTa();
    }

    static String tenGiaLap(double tanSo) { return "Giả lập (sin " + KhachAmThanh.fmtG(tanSo) + " Hz)"; }

    /** Định dạng little-endian: nhiều trình điều khiển Windows chỉ nhận kiểu này; khi đó tự đảo byte. */
    static final AudioFormat DINH_DANG_LE = new AudioFormat(AmThanh.TAN_SO_MAU, 16, 1, true, false);

    /** Tên các bộ trộn (mixer) có đường thu (TargetDataLine) hỗ trợ 16 kHz/16 bit/mono. */
    static List<String> dsMicro() {
        List<String> ds = new ArrayList<>();
        for (Mixer.Info mi : AudioSystem.getMixerInfo()) {
            try {
                Mixer m = AudioSystem.getMixer(mi);
                if (m.isLineSupported(new DataLine.Info(TargetDataLine.class, AmThanh.DINH_DANG))
                        || m.isLineSupported(new DataLine.Info(TargetDataLine.class, DINH_DANG_LE))) ds.add(mi.getName());
            } catch (RuntimeException ignored) { }       // một số trình điều khiển ném lỗi khi hỏi
        }
        return ds;
    }

    static boolean coLoa() {
        try {
            return AudioSystem.isLineSupported(new DataLine.Info(SourceDataLine.class, AmThanh.DINH_DANG))
                    || AudioSystem.isLineSupported(new DataLine.Info(SourceDataLine.class, DINH_DANG_LE));
        } catch (RuntimeException e) {
            return false;
        }
    }

    /** Nhân mức (Mic Volume, âm lượng nghe…) và cắt ngọn; trả mảng mới. */
    static short[] nhan(short[] mau, double heSo) {
        short[] ra = new short[mau.length];
        for (int i = 0; i < mau.length; i++) ra[i] = AmThanh.catNgon(mau[i] * heSo, null);
        return ra;
    }

    static byte[] sangByte(short[] mau, boolean bigEndian) {
        byte[] b = new byte[mau.length * 2];
        for (int i = 0; i < mau.length; i++) {
            byte cao = (byte) (mau[i] >> 8), thap = (byte) mau[i];
            b[2 * i] = bigEndian ? cao : thap;
            b[2 * i + 1] = bigEndian ? thap : cao;
        }
        return b;
    }

    static short[] sangMau(byte[] b, boolean bigEndian) {
        short[] m = new short[b.length / 2];
        for (int i = 0; i < m.length; i++) {
            int cao = bigEndian ? b[2 * i] : b[2 * i + 1], thap = (bigEndian ? b[2 * i + 1] : b[2 * i]) & 0xFF;
            m[i] = (short) ((cao << 8) | thap);
        }
        return m;
    }

    // ---------------------------------------------------------------- Nguồn giả lập

    /** Sin tần số f, biên độ a (0…1 toàn thang), pha liên tục giữa các khung – như nguồn của bản dòng lệnh. */
    static final class NguonSin implements NguonKhung {
        final KhachAmThanh.NguonSin sin;
        final double f;
        NguonSin(double f, double a) { this.f = f; sin = new KhachAmThanh.NguonSin(f, a * 32767); }
        public short[] layKhung() { return sin.layKhung(); }
        public void dong() { }
        public boolean that() { return false; }
        public String moTa() { return tenGiaLap(f); }
    }

    // ---------------------------------------------------------------- Micro thật

    static final class NguonMic implements NguonKhung {
        final TargetDataLine line;
        final boolean bigEndian;
        final String ten;
        final ArrayBlockingQueue<short[]> hang = new ArrayBlockingQueue<>(8);   // tối đa 160 ms chờ
        volatile boolean chay = true;

        NguonMic(TargetDataLine line, boolean bigEndian, String ten) {
            this.line = line;
            this.bigEndian = bigEndian;
            this.ten = ten;
            Thread.ofPlatform().daemon().name("thu-micro").start(this::vongThu);
        }

        /** Luồng thu: read() chặn tới khi đủ 640 byte (20 ms âm thanh). */
        private void vongThu() {
            byte[] b = new byte[AmThanh.BYTE_MOI_KHUNG];
            while (chay) {
                int n = line.read(b, 0, b.length);
                if (n <= 0) {
                    if (!line.isOpen()) return;
                    continue;
                }
                if (n < b.length) continue;               // khung thiếu (khi đang đóng): bỏ
                short[] k = sangMau(b, bigEndian);
                if (!hang.offer(k)) {                     // đầy: bỏ khung cũ nhất để không trễ dần
                    hang.poll();
                    hang.offer(k);
                }
            }
        }

        public short[] layKhung() { return hang.poll(); }

        public void dong() {
            chay = false;
            line.stop();
            line.close();
        }

        public boolean that() { return true; }
        public String moTa() { return "micro thật: " + ten; }
    }

    /** Mở micro theo tên bộ trộn (thử big-endian rồi little-endian). */
    static NguonKhung moMicro(String tenMixer) throws LineUnavailableException {
        for (Mixer.Info mi : AudioSystem.getMixerInfo()) {
            if (!mi.getName().equals(tenMixer)) continue;
            Mixer m = AudioSystem.getMixer(mi);
            for (AudioFormat f : new AudioFormat[] {AmThanh.DINH_DANG, DINH_DANG_LE}) {
                DataLine.Info info = new DataLine.Info(TargetDataLine.class, f);
                if (!m.isLineSupported(info)) continue;
                TargetDataLine t = (TargetDataLine) m.getLine(info);
                t.open(f, AmThanh.BYTE_MOI_KHUNG * 4);
                t.start();
                return new NguonMic(t, f.isBigEndian(), tenMixer);
            }
        }
        throw new LineUnavailableException("không tìm thấy micro " + tenMixer);
    }

    /**
     * Mở nguồn theo lựa chọn trong ô chọn thiết bị. Lựa chọn "Giả lập" hoặc micro thật không mở được -> sin;
     * baoLoi nhận lý do để giao diện hiện cho người dùng.
     */
    static NguonKhung moNguon(String luaChon, double tanSoGiaLap, java.util.function.Consumer<String> baoLoi) {
        if (luaChon != null && !luaChon.startsWith("Giả lập")) {
            try {
                return moMicro(luaChon);
            } catch (LineUnavailableException | RuntimeException e) {
                baoLoi.accept("Không mở được micro \"" + luaChon + "\" (" + e.getMessage() + ") – chuyển sang giả lập");
            }
        }
        return new NguonSin(tanSoGiaLap, 0.25);
    }

    // ---------------------------------------------------------------- Loa thật

    static final class LoaPhat {
        final SourceDataLine line;
        final boolean bigEndian;
        int boKhung;                                      // số khung bỏ vì bộ đệm loa đầy

        private LoaPhat(SourceDataLine line, boolean bigEndian) { this.line = line; this.bigEndian = bigEndian; }

        /** Mở loa mặc định; null nếu máy không có loa (vẫn chạy, chỉ đo mức). */
        static LoaPhat mo() {
            for (AudioFormat f : new AudioFormat[] {AmThanh.DINH_DANG, DINH_DANG_LE}) {
                try {
                    SourceDataLine l = AudioSystem.getSourceDataLine(f);
                    l.open(f, AmThanh.BYTE_MOI_KHUNG * 8);    // 160 ms đệm
                    l.start();
                    return new LoaPhat(l, f.isBigEndian());
                } catch (LineUnavailableException | IllegalArgumentException ignored) { }
            }
            return null;
        }

        /** Ghi một khung nếu loa còn chỗ – không bao giờ chặn. */
        void phat(short[] mau) {
            byte[] b = sangByte(mau, bigEndian);
            if (line.available() >= b.length) line.write(b, 0, b.length);
            else boKhung++;
        }

        void dong() {
            line.stop();
            line.close();
        }
    }
}
