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
 * Bài mở rộng MR6.5 – Thiết bị âm thanh: micro/loa thật hoặc nguồn giả lập. Chép từ ThietBiAmThanh.java của MR6.2,
 * sửa: micro giả lập là NguonAmClapper (440 Hz + bíp 1000 Hz mỗi giây tròn). Chỉ dùng javax.sound.sampled của JDK.
 *
 *  - Micro thật: TargetDataLine 16 kHz, 16 bit, mono. Một luồng nền đọc liên tục từng khung 640 byte (20 ms) và đặt
 *    vào hàng đợi nhỏ; vòng 20 ms của máy phát chỉ lấy ra (không bao giờ chờ thiết bị) – mô hình nhà sản xuất –
 *    người tiêu dùng.
 *  - Loa thật: SourceDataLine; chỉ ghi khi bộ đệm của loa còn chỗ, đầy thì bỏ khung (không chặn nhịp 20 ms).
 *  - Không có thiết bị (máy chủ không có card âm thanh, container…) hoặc chọn giả lập: nguồn clapper; loa: chỉ đo mức.
 * Mọi nguồn âm có cùng giao diện: layKhung() -> 320 mẫu, dong(); that() (true = thiết bị thật) và moTa() để cửa sổ
 * hiện chỉ báo rõ ràng.
 */
final class ThietBiAmThanh {
    private ThietBiAmThanh() {}

    static final String TEN_GIA_LAP = "Micro giả lập (440 Hz + bíp mỗi giây)";
    static final AudioFormat DINH_DANG = new AudioFormat(Chung.TAN_SO_MAU, 16, 1, true, true);      // như RTP L16
    /** Little-endian: nhiều trình điều khiển Windows chỉ nhận kiểu này; khi đó tự đảo byte. */
    static final AudioFormat DINH_DANG_LE = new AudioFormat(Chung.TAN_SO_MAU, 16, 1, true, false);

    interface NguonAm {
        short[] layKhung();                 // luôn 320 mẫu
        void dong();
        boolean that();
        String moTa();
    }

    /** Micro giả lập: 440 Hz biên độ thấp + bíp 1000 Hz 100 ms ở mỗi giây tròn. */
    static final class MicGiaLap extends Chung.NguonAmClapper implements NguonAm {
        public void dong() { }
        public boolean that() { return false; }
        public String moTa() { return "GIẢ LẬP – 440 Hz + bíp 1000 Hz mỗi giây"; }
    }

    /** Tên các bộ trộn (mixer) có đường thu hỗ trợ 16 kHz/16 bit/mono. */
    static List<String> dsMicro() {
        List<String> ds = new ArrayList<>();
        try {
            for (Mixer.Info mi : AudioSystem.getMixerInfo()) {
                try {
                    Mixer m = AudioSystem.getMixer(mi);
                    if (m.isLineSupported(new DataLine.Info(TargetDataLine.class, DINH_DANG))
                            || m.isLineSupported(new DataLine.Info(TargetDataLine.class, DINH_DANG_LE))) ds.add(mi.getName());
                } catch (RuntimeException ignored) { }      // một số trình điều khiển ném lỗi khi hỏi
            }
        } catch (RuntimeException ignored) { }
        return ds;
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

    /** Micro thật: luồng thu đọc 640 byte mỗi lần (read() chặn tới khi đủ 20 ms âm thanh). */
    static final class MicThat implements NguonAm {
        final TargetDataLine line;
        final boolean bigEndian;
        final String ten;
        final ArrayBlockingQueue<short[]> hang = new ArrayBlockingQueue<>(10);   // tối đa 200 ms chờ
        volatile boolean chay = true;

        MicThat(TargetDataLine line, boolean bigEndian, String ten) {
            this.line = line;
            this.bigEndian = bigEndian;
            this.ten = ten;
            Chung.chayNen("thu-micro", this::vongThu);
        }

        private void vongThu() {
            byte[] b = new byte[Chung.BYTE_MOI_KHUNG];
            while (chay) {
                int n = line.read(b, 0, b.length);
                if (n <= 0) {
                    if (!line.isOpen()) return;
                    continue;
                }
                if (n < b.length) continue;                       // khung thiếu (khi đang đóng): bỏ
                hang.offer(sangMau(b, bigEndian));                 // đầy thì bỏ khung mới
            }
        }

        public short[] layKhung() {
            short[] k = hang.poll();
            return k != null ? k : new short[Chung.MAU_MOI_KHUNG];   // chưa có dữ liệu: coi như im lặng
        }

        public void dong() {
            chay = false;
            line.stop();
            line.close();
        }

        public boolean that() { return true; }
        public String moTa() { return "THẬT – " + ten; }
    }

    /** Mở micro theo tên bộ trộn (thử big-endian rồi little-endian). */
    static MicThat moMicThat(String tenMixer) throws LineUnavailableException {
        for (Mixer.Info mi : AudioSystem.getMixerInfo()) {
            if (!mi.getName().equals(tenMixer)) continue;
            Mixer m = AudioSystem.getMixer(mi);
            for (AudioFormat f : new AudioFormat[] {DINH_DANG, DINH_DANG_LE}) {
                DataLine.Info info = new DataLine.Info(TargetDataLine.class, f);
                if (!m.isLineSupported(info)) continue;
                TargetDataLine t = (TargetDataLine) m.getLine(info);
                t.open(f, Chung.BYTE_MOI_KHUNG * 4);
                t.start();
                return new MicThat(t, f.isBigEndian(), tenMixer);
            }
        }
        throw new LineUnavailableException("không tìm thấy micro " + tenMixer);
    }

    record KetQuaMic(NguonAm mic, String loi) {}

    /** Mở micro đã chọn; tenMixer = null (giả lập) hoặc lỗi mở thiết bị thật -> nguồn giả lập, kèm thông báo lỗi. */
    static KetQuaMic moMic(String tenMixer) {
        if (tenMixer != null) {
            try {
                return new KetQuaMic(moMicThat(tenMixer), "");
            } catch (LineUnavailableException | RuntimeException e) {
                return new KetQuaMic(new MicGiaLap(), "không mở được micro '" + tenMixer + "' (" + e.getMessage()
                        + "); chuyển sang giả lập");
            }
        }
        return new KetQuaMic(new MicGiaLap(), "");
    }

    /** Loa thật (thiết bị phát mặc định): vòng 20 ms ghi khung nếu còn chỗ – không bao giờ chặn. */
    static final class LoaThat {
        final SourceDataLine line;
        final boolean bigEndian;
        int boKhung;

        LoaThat(SourceDataLine line, boolean bigEndian) { this.line = line; this.bigEndian = bigEndian; }

        void phat(short[] mau) {
            byte[] b = sangByte(mau, bigEndian);
            if (line.available() >= b.length) line.write(b, 0, b.length);
            else boKhung++;
        }

        void dong() {
            try {
                line.stop();
                line.close();
            } catch (RuntimeException ignored) { }
        }
    }

    record KetQuaLoa(LoaThat loa, String lyDo) {}

    /** Mở loa mặc định; loa = null kèm lý do nếu máy không có thiết bị phát (vẫn chạy, chỉ đo mức). */
    static KetQuaLoa moLoa() {
        String loi = null;
        for (AudioFormat f : new AudioFormat[] {DINH_DANG, DINH_DANG_LE}) {
            try {
                if (!AudioSystem.isLineSupported(new DataLine.Info(SourceDataLine.class, f))) continue;
                SourceDataLine l = AudioSystem.getSourceDataLine(f);
                l.open(f, Chung.BYTE_MOI_KHUNG * 8);        // 160 ms đệm
                l.start();
                return new KetQuaLoa(new LoaThat(l, f.isBigEndian()), "");
            } catch (LineUnavailableException | RuntimeException e) {
                loi = e.getMessage();
            }
        }
        return new KetQuaLoa(null, loi == null ? "không có thiết bị phát – chỉ đo mức"
                : "không mở được loa (" + loi + ") – chỉ đo mức");
    }
}
