import java.io.BufferedReader;
import java.io.ByteArrayInputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.ByteOrder;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Map;
import java.util.function.Predicate;
import javax.sound.sampled.AudioFileFormat;
import javax.sound.sampled.AudioFormat;
import javax.sound.sampled.AudioInputStream;
import javax.sound.sampled.AudioSystem;
import javax.sound.sampled.UnsupportedAudioFileException;

/**
 * Các thủ tục dùng chung cho server và client hội thoại âm thanh.
 *
 * Âm thanh số PCM (Pulse-Code Modulation) 16 bit, mono, tần số lấy mẫu 16000 Hz:
 *  - lấy mẫu: mỗi giây đo biên độ sóng âm 16000 lần; mỗi mẫu là số nguyên 16 bit có dấu
 *    (-32768 … 32767). Theo định lý Nyquist, 16 kHz biểu diễn được âm tới 8 kHz – đủ cho giọng nói;
 *  - tốc độ bit: 16000 mẫu/s × 16 bit = 256 kbit/s (32000 byte/s);
 *  - khung 20 ms: 16000 × 0,020 = 320 mẫu = 640 byte -> 50 gói mỗi giây;
 *  - mỗi gói thêm 12 byte RTP + 8 byte UDP + 20 byte IPv4 = 40 byte phụ phí
 *    -> 50 × 40 × 8 = 16 kbit/s, tổng ≈ 272 kbit/s mỗi chiều cho mỗi client.
 * Mức âm đo bằng dBFS (decibel so với toàn thang): 0 dBFS là mức RMS lớn nhất (32768). Sóng sin
 * biên độ 0,25 toàn thang có RMS 0,25/√2 -> -15,1 dBFS; nhân biên độ 2 -> +6,02 dB.
 */
final class AmThanh {
    private AmThanh() {}

    static final int TAN_SO_MAU = 16000;                       // mẫu/giây
    static final int KHUNG_MS = 20;                            // độ dài một khung (gói) âm thanh
    static final int MAU_MOI_KHUNG = TAN_SO_MAU * KHUNG_MS / 1000;   // 320 mẫu
    static final int BYTE_MOI_KHUNG = MAU_MOI_KHUNG * 2;       // 640 byte
    static final int PT_AM_THANH = 96;                         // payload type động (RFC 3551) cho L16/16000/mono
    static final int PT_GIU_KET_NOI = 127;                     // gói rỗng: đăng ký địa chỉ UDP, giữ kết nối khi mic tắt
    static final int SSRC_SERVER = 0;                          // mã nguồn của luồng trộn do server gửi
    /** Định dạng âm thanh của javax.sound.sampled: 16 kHz, 16 bit, mono, có dấu, big-endian (như RTP L16). */
    static final AudioFormat DINH_DANG = new AudioFormat(TAN_SO_MAU, 16, 1, true, true);
    static final long T0 = System.nanoTime();                  // mốc thời gian của chương trình

    static double giay() { return (System.nanoTime() - T0) / 1e9; }

    // ---------------------------------------------------------------- Gói RTP (RFC 3550)

    /** Header cố định 12 byte: V=2|P|X|CC, M|PT, số thứ tự 16 bit, nhãn thời gian 32 bit (đơn vị: mẫu), SSRC 32 bit. */
    static byte[] dongGoiRTP(int pt, int seq, long ts, long ssrc, byte[] payload) {
        ByteBuffer b = ByteBuffer.allocate(12 + payload.length);       // big-endian = thứ tự byte mạng
        b.put((byte) 0x80).put((byte) (pt & 0x7F)).putShort((short) seq).putInt((int) ts).putInt((int) ssrc).put(payload);
        return b.array();
    }

    record GoiRTP(int pt, int seq, long ts, long ssrc, byte[] payload) {}

    /** Tách gói RTP; null nếu không hợp lệ. */
    static GoiRTP tachRTP(byte[] b, int len) {
        if (len < 12 || ((b[0] & 0xFF) >> 6) != 2) return null;       // phiên bản RTP phải là 2
        ByteBuffer bb = ByteBuffer.wrap(b, 0, len);
        byte[] payload = new byte[len - 12];
        System.arraycopy(b, 12, payload, 0, payload.length);
        return new GoiRTP(b[1] & 0x7F, bb.getShort(2) & 0xFFFF, bb.getInt(4) & 0xFFFFFFFFL, bb.getInt(8) & 0xFFFFFFFFL, payload);
    }

    // ---------------------------------------------------------------- PCM <-> byte

    /** RTP truyền L16 theo thứ tự byte mạng (big-endian), RFC 3551 mục 4.5.11. */
    static byte[] pcmSangByte(short[] mau) {
        ByteBuffer b = ByteBuffer.allocate(2 * mau.length);
        for (short x : mau) b.putShort(x);
        return b.array();
    }

    static short[] byteSangPCM(byte[] b) {
        short[] m = new short[b.length / 2];
        ByteBuffer bb = ByteBuffer.wrap(b);
        for (int i = 0; i < m.length; i++) m[i] = bb.getShort();
        return m;
    }

    /** Cắt ngọn (clipping): giá trị vượt phạm vi 16 bit bị giữ ở biên. Trả về mẫu; cờ cắt qua mảng 1 phần tử. */
    static short catNgon(double x, boolean[] cat) {
        double v = Math.rint(x);                                     // làm tròn tới số nguyên gần nhất
        if (v > 32767) { if (cat != null) cat[0] = true; return 32767; }
        if (v < -32768) { if (cat != null) cat[0] = true; return -32768; }
        if (cat != null) cat[0] = false;
        return (short) v;
    }

    // ---------------------------------------------------------------- Mức âm

    /** Mức RMS (căn bậc hai của trung bình bình phương) theo dBFS. */
    static double mucDBFS(short[] mau, int n) {
        if (n == 0) return Double.NEGATIVE_INFINITY;
        double tong = 0;
        for (int i = 0; i < n; i++) tong += (double) mau[i] * mau[i];
        double rms = Math.sqrt(tong / n);
        return rms == 0 ? Double.NEGATIVE_INFINITY : 20 * Math.log10(rms / 32768);
    }

    /** Thanh đo mức dạng chữ: -60 dBFS (trống) … 0 dBFS (đầy). */
    static String thanhMuc(double db, int rong) {
        int n = 0;
        if (db != Double.NEGATIVE_INFINITY) n = Math.max(0, Math.min(rong, (int) Math.round((db + 60) / 60 * rong)));
        return "|" + "#".repeat(n) + "-".repeat(rong - n) + "|";
    }

    static String chuDB(double db, int rong) {
        if (db < -90) return String.format("%" + rong + "s", "---");
        return String.format(java.util.Locale.ROOT, "%" + rong + ".1f", db);
    }

    // ---------------------------------------------------------------- Bộ đệm jitter

    /**
     * Bộ đệm chống rung trễ cho một luồng RTP. Giữ vài khung (doSau) trước khi bắt đầu phát, rồi cứ mỗi 20 ms
     * lấy đúng khung có số thứ tự kế tiếp:
     *  - có khung -> phát; thiếu khung nhưng đã có khung sau -> coi như mất, phát khoảng lặng;
     *  - bộ đệm rỗng -> cạn, chờ nạp lại doSau khung; khung đến sau lượt phát -> trễ, bỏ.
     * Đồng thời thống kê theo RFC 3550: số gói mất (khoảng trống số thứ tự) và jitter (trung bình
     * trượt 1/16 của chênh lệch thời gian truyền giữa hai gói liên tiếp). Không an toàn luồng: người gọi giữ khóa.
     */
    static final class BoDemJitter {
        final int doSau, toiDa = 50;
        final Map<Long, byte[]> khung = new HashMap<>();             // số thứ tự mở rộng -> payload
        long keTiep;
        boolean dangPhat, coSeq;
        long seqLonNhat, seqDau;
        int nhan, tre, thieu, can, tran;
        double jitter;                                             // đơn vị: mẫu (1 ms = 16 mẫu)
        double truyenTruoc;
        boolean coTruyen;

        BoDemJitter(int doSau) { this.doSau = doSau; }

        /** Số thứ tự 16 bit quay vòng sau 65535; đổi thành số nguyên tăng mãi. */
        long moRong(int seq) {
            if (!coSeq) return seq;
            long lech = (seq - (seqLonNhat & 0xFFFF)) & 0xFFFF;
            return lech < 0x8000 ? seqLonNhat + lech : seqLonNhat - (0x10000 - lech);
        }

        void nhanGoi(int seq, long ts, byte[] payload, double lucDen) {
            long s = moRong(seq);
            if (!coSeq) {
                coSeq = true;
                seqDau = s;
                seqLonNhat = s;
            }
            if (s > seqLonNhat) seqLonNhat = s;
            nhan++;
            double truyen = lucDen * TAN_SO_MAU - ts;                // thời gian truyền (lệch hằng số không quan trọng)
            if (coTruyen) {
                double d = Math.abs(truyen - truyenTruoc);
                if (d < TAN_SO_MAU) jitter += (d - jitter) / 16;     // bỏ qua bước nhảy lớn
            }
            truyenTruoc = truyen;
            coTruyen = true;
            if (dangPhat && s < keTiep) {
                tre++;                                             // đến quá muộn, đã qua lượt phát
                return;
            }
            khung.put(s, payload.clone());
            if (khung.size() > toiDa) {                            // tràn: bỏ khung cũ nhất
                khung.remove(nhoNhat());
                tran++;
            }
        }

        long nhoNhat() { return khung.keySet().stream().mapToLong(Long::longValue).min().orElse(0); }

        /** Gọi mỗi 20 ms; trả về payload của khung cần phát hoặc null (phát khoảng lặng). */
        byte[] lay() {
            if (!dangPhat) {
                if (khung.size() < doSau) return null;
                dangPhat = true;
                keTiep = nhoNhat();
            }
            byte[] p = khung.remove(keTiep);
            if (p != null) {
                keTiep++;
                return p;
            }
            if (khung.isEmpty()) {                                 // cạn bộ đệm: chờ nạp lại
                dangPhat = false;
                can++;
                return null;
            }
            thieu++;                                               // mất khung này, đã có khung sau
            keTiep++;
            return null;
        }

        void xoa() {
            khung.clear();
            dangPhat = false;
        }

        /** Số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận. */
        int mat() { return coSeq ? (int) Math.max(0, seqLonNhat - seqDau + 1 - nhan) : 0; }

        double jitterMs() { return jitter / (TAN_SO_MAU / 1000.0); }
    }

    // ---------------------------------------------------------------- Phân tích tần số

    /**
     * Biên độ thành phần tần số f trong một khối mẫu, bằng thuật toán Goertzel:
     *   s[n] = x[n] + 2cos(ω)·s[n-1] - s[n-2],  ω = 2π·f/fs;  |X|² = s1² + s2² - 2cos(ω)·s1·s2
     * Tín hiệu nhân trước với cửa sổ Hann để giảm rò phổ; biên độ sin A ≈ 2|X| / Σw.
     */
    static double goertzelBienDo(short[] mau, int from, int n, double f) {
        double w = 2 * Math.PI * f / TAN_SO_MAU, heSo = 2 * Math.cos(w);
        double s1 = 0, s2 = 0, tongW = 0, k = 2 * Math.PI / (n - 1);
        for (int i = 0; i < n; i++) {
            double cs = 0.5 - 0.5 * Math.cos(k * i);               // cửa sổ Hann
            tongW += cs;
            double s0 = mau[from + i] * cs + heSo * s1 - s2;
            s2 = s1;
            s1 = s0;
        }
        return 2 * Math.sqrt(Math.max(s1 * s1 + s2 * s2 - heSo * s1 * s2, 0)) / tongW;
    }

    /**
     * Mức RMS (dBFS) của một âm trong đoạn dài: chia khối 0,1 s (độ phân giải 10 Hz), đo biên độ từng khối
     * rồi lấy trung bình công suất – không bị sai khi pha của âm nhảy giữa đoạn.
     */
    static double mucAmDBFS(short[] mau, int from, int to, double f) {
        final int khoi = 1600;
        double tong = 0;
        int n = 0;
        for (int i = from; i + khoi <= to; i += khoi) {
            double a = goertzelBienDo(mau, i, khoi, f);
            tong += a * a;
            n++;
        }
        if (n == 0) return Double.NEGATIVE_INFINITY;
        double rms = Math.sqrt(tong / n) / Math.sqrt(2);            // biên độ sin -> RMS
        return rms <= 0.5 ? Double.NEGATIVE_INFINITY : 20 * Math.log10(rms / 32768);
    }

    // ---------------------------------------------------------------- Tệp WAV (javax.sound.sampled)

    /** Ghi WAV PCM 16 bit mono 16 kHz bằng AudioSystem.write (header RIFF do JDK tạo, mẫu little-endian). */
    static void ghiWAV(File tep, short[] mau, int n) throws IOException {
        ByteBuffer b = ByteBuffer.allocate(2 * n).order(ByteOrder.LITTLE_ENDIAN);   // WAV lưu little-endian
        for (int i = 0; i < n; i++) b.putShort(mau[i]);
        AudioFormat le = new AudioFormat(TAN_SO_MAU, 16, 1, true, false);
        try (AudioInputStream ais = new AudioInputStream(new ByteArrayInputStream(b.array()), le, n)) {
            AudioSystem.write(ais, AudioFileFormat.Type.WAVE, tep);
        }
    }

    /** Đọc WAV PCM 16 bit mono 16000 Hz bằng AudioSystem.getAudioInputStream. */
    static short[] docWAV(File tep) throws IOException {
        try (AudioInputStream ais = AudioSystem.getAudioInputStream(tep)) {
            AudioFormat f = ais.getFormat();
            if (f.getEncoding() != AudioFormat.Encoding.PCM_SIGNED || f.getChannels() != 1
                    || f.getSampleRate() != TAN_SO_MAU || f.getSampleSizeInBits() != 16)
                throw new IOException("cần WAV PCM 16 bit, mono, 16000 Hz");
            byte[] data = ais.readAllBytes();
            ByteBuffer bb = ByteBuffer.wrap(data).order(f.isBigEndian() ? ByteOrder.BIG_ENDIAN : ByteOrder.LITTLE_ENDIAN);
            short[] m = new short[data.length / 2];
            for (int i = 0; i < m.length; i++) m[i] = bb.getShort();
            return m;
        } catch (UnsupportedAudioFileException e) {
            throw new IOException("không phải tệp WAV");
        }
    }

    // ---------------------------------------------------------------- Kênh điều khiển JSON

    /** Mỗi thông điệp điều khiển là một dòng JSON kết thúc bằng '\n' (định khung theo dòng). */
    static void guiJSON(Socket s, Map<String, Object> v) {
        try {
            OutputStream out = s.getOutputStream();
            synchronized (s) { out.write((Json.stringify(v) + "\n").getBytes(StandardCharsets.UTF_8)); }
        } catch (IOException ignored) { }
    }

    /**
     * Đọc từng dòng lệnh từ stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy; phần sau dấu # là
     * chú thích. Hàm lenh trả về false để dừng; khi đó chayKichBan trả về true.
     */
    static boolean chayKichBan(Predicate<String> lenh) {
        try (BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                String dong = line.split("#", 2)[0].strip();
                if (dong.startsWith("@")) {
                    String[] p = dong.split(" ", 2);
                    double t = Double.parseDouble(p[0].substring(1));
                    long ms = (long) ((t - giay()) * 1000);
                    if (ms > 0) Thread.sleep(ms);
                    dong = p.length > 1 ? p[1].strip() : "";
                }
                if (!dong.isEmpty() && !lenh.test(dong)) return true;
            }
        } catch (IOException | InterruptedException ignored) { }
        return false;
    }
}
