import java.awt.image.BufferedImage;
import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.PrintStream;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.HashSet;
import java.util.Iterator;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;
import java.util.Set;
import java.util.TreeMap;
import java.util.concurrent.locks.LockSupport;
import java.util.function.Consumer;
import java.util.zip.DataFormatException;
import java.util.zip.Deflater;
import java.util.zip.Inflater;
import javax.imageio.ImageIO;

/**
 * Bài mở rộng MR6.5 – Các thủ tục dùng chung cho máy chủ, máy phát và người xem (phát trực tiếp 1→n).
 *
 * Tự chứa (không dùng mã của bài khác): chép và mở rộng phần cần thiết của MR6.2 (âm thanh RTP, bộ đệm
 * jitter) và MR6.3 (header video, chia mảnh, ghép khung, phông chữ 5×7, PNG).
 *
 * Hai loại datagram trên CÙNG một socket UDP, phân biệt bằng byte đầu tiên:
 *   - 0x80: gói RTP âm thanh (RFC 3550): V=2, P=0, X=0, CC=0 -> byte đầu luôn là 0x80;
 *   - 0x01: gói video của bài (phiên bản 1): loại 2 = giữ kết nối, 3 = mảnh khung I, 4 = mảnh khung P.
 *
 * Nén liên khung (inter-frame) đơn giản – ý tưởng của mọi codec video (H.264, VP8, AV1):
 *   - khung khóa I (intra): nén một mình  ->  I = zlib(rgb thô);
 *   - khung sai khác P (predicted): chỉ mã hóa phần THAY ĐỔI so với khung ngay trước
 *         P = zlib(XOR từng byte(khung hiện tại, khung trước))
 *     cảnh tĩnh -> XOR gần như toàn 0 -> nén còn vài trăm byte. Giải mã P cần đúng khung trước:
 *     mất một khung là hỏng cả chuỗi tới khung I kế tiếp (nhóm khung – GOP, Group Of Pictures).
 * Định dạng gói và JSON điều khiển giống hệt bản Python, C#, Rust, Go: các bản chạy chung được với nhau.
 */
final class Chung {
    private Chung() {}

    // ---------------------------------------------------------------- Âm thanh (như MR6.2)
    static final int TAN_SO_MAU = 16000;                              // mẫu/giây
    static final int KHUNG_MS = 20;                                   // một gói âm thanh = 20 ms
    static final int MAU_MOI_KHUNG = TAN_SO_MAU * KHUNG_MS / 1000;    // 320 mẫu
    static final int BYTE_MOI_KHUNG = MAU_MOI_KHUNG * 2;              // 640 byte (L16)
    static final int PT_AM_THANH = 96;                                // payload type động (RFC 3551) cho L16/16000/mono
    static final int CO_RTP = 12;                                     // header RTP 12 byte: V|P|X|CC, M|PT, seq, ts, ssrc
    static final int DAU_RTP = 0x80;
    static final double MS_MOI_MAU = 1000.0 / TAN_SO_MAU;

    // ---------------------------------------------------------------- Video (như MR6.3)
    static final int RONG = 160, CAO = 120;
    static final int FPS_MAC_DINH = 10, GOP_MAC_DINH = 20;
    static final int MAX_DATAGRAM = 1200, CO_HEADER = 20;             // header !BBHIHHHHI = 20 byte
    static final int DU_LIEU_TOI_DA = MAX_DATAGRAM - CO_HEADER;       // 1180 byte
    static final int PHIEN_BAN = 1;                                   // cũng là byte đầu của gói video (0x01)
    static final int LOAI_GIU = 2, LOAI_I = 3, LOAI_P = 4;
    static final int MANH_TOI_DA = 256;

    // ---------------------------------------------------------------- Giới hạn, thời gian chờ
    static final int GIOI_HAN_DONG = 64 * 1024;       // một dòng JSON điều khiển tối đa 64 KB
    static final int BO_DEM_GOP_TOI_DA = 4 << 20;     // bộ đệm GOP ở server tối đa 4 MB
    static final double PLI_SERVER_S = 0.5;           // server chuyển xin_khung_khoa tối đa 1 lần / 500 ms
    static final double PLI_XEM_S = 1.0;              // người xem gửi xin_khung_khoa tối đa 1 lần / 1 s
    static final double CHO_THAM_CHIEU_S = 0.2;       // chờ khung thiếu trước khi coi là mất tham chiếu
    static final double CHO_MANH_S = 0.5;             // chờ đủ mảnh của một khung
    static final int HANG_HIEN_THI = 30;              // hàng khung chờ hiển thị tối đa 30 khung
    static final int DO_SAU_MAC_DINH = 5;             // bộ đệm jitter 5 khung = 100 ms

    // ---------------------------------------------------------------- Đồng hồ, in, định dạng
    static final long T0 = System.nanoTime();

    /** Đồng hồ đơn điệu (giây) – như time.monotonic() của Python. */
    static double bayGio() { return (System.nanoTime() - T0) / 1e9; }

    /** Giờ thực (ms Unix) – như time.time() * 1000. */
    static long gioUnixMs() { return System.currentTimeMillis(); }

    /** Ngủ s giây (độ chính xác dưới 1 ms nhờ LockSupport.parkNanos). */
    static void ngu(double s) { if (s > 0) LockSupport.parkNanos((long) (s * 1e9)); }

    static String f(String mau, Object... thamSo) { return String.format(Locale.ROOT, mau, thamSo); }

    /** stdout UTF-8, tự xả mỗi dòng; println của PrintStream đã đồng bộ nên mỗi dòng in trọn vẹn. */
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);

    /** Hàm in có tiền tố, dùng chung một khóa: nhiều luồng cùng in vẫn không chen giữa dòng. */
    static Consumer<String> nguoiIn(String tienTo) {
        return text -> { synchronized (OUT) { OUT.println(tienTo + text); } };
    }

    // ================================================================ RTP âm thanh
    /** Header RTP 12 byte: 0x80 (V=2), PT=96 (M=0), số thứ tự, nhãn thời gian (mẫu), SSRC. */
    static byte[] dongGoiRtp(int seq, long ts, long ssrc, byte[] payload) {
        ByteBuffer b = ByteBuffer.allocate(CO_RTP + payload.length);       // big-endian = thứ tự byte mạng
        b.put((byte) DAU_RTP).put((byte) PT_AM_THANH).putShort((short) seq).putInt((int) ts).putInt((int) ssrc).put(payload);
        return b.array();
    }

    record Rtp(int pt, int seq, long ts, long ssrc, byte[] payload) {}

    /** Tách gói RTP; null nếu sai dạng. */
    static Rtp tachRtp(byte[] g, int len) {
        if (len < CO_RTP || ((g[0] & 0xFF) >> 6) != 2) return null;
        ByteBuffer b = ByteBuffer.wrap(g, 0, len);
        byte[] p = new byte[len - CO_RTP];
        System.arraycopy(g, CO_RTP, p, 0, p.length);
        return new Rtp(g[1] & 0x7F, b.getShort(2) & 0xFFFF, b.getInt(4) & 0xFFFFFFFFL, b.getInt(8) & 0xFFFFFFFFL, p);
    }

    /** L16 truyền theo thứ tự byte mạng (big-endian) – RFC 3551 mục 4.5.11. */
    static byte[] pcmSangByte(short[] mau) {
        byte[] b = new byte[mau.length * 2];
        for (int i = 0; i < mau.length; i++) {
            b[2 * i] = (byte) (mau[i] >> 8);
            b[2 * i + 1] = (byte) mau[i];
        }
        return b;
    }

    static short[] byteSangPcm(byte[] b) {
        short[] m = new short[b.length / 2];
        for (int i = 0; i < m.length; i++) m[i] = (short) ((b[2 * i] << 8) | (b[2 * i + 1] & 0xFF));
        return m;
    }

    /** Mức RMS theo dBFS (0 dBFS = toàn thang 32768). */
    static double mucDbfs(short[] mau) {
        if (mau == null || mau.length == 0) return Double.NEGATIVE_INFINITY;
        double s = 0;
        for (short x : mau) s += (double) x * x;
        double rms = Math.sqrt(s / mau.length);
        return rms > 0 ? 20 * Math.log10(rms / 32768) : Double.NEGATIVE_INFINITY;
    }

    /**
     * Bộ đệm chống rung trễ (jitter buffer) cho luồng RTP – như MR6.2, thêm nhãn thời gian.
     * Giữ doSau khung trước khi bắt đầu phát, rồi cứ 20 ms lấy đúng khung có số thứ tự kế tiếp:
     *   - có khung  -> trả về (ts, payload);
     *   - thiếu khung nhưng đã có khung sau -> mất: trả về (ts ước lượng, null) – phát im lặng,
     *     đồng hồ phát vẫn tiến 20 ms;
     *   - rỗng -> cạn: trả về null (đồng hồ phát dừng), chờ nạp lại doSau khung.
     * Không an toàn luồng: người gọi giữ khóa.
     */
    static final class BoDemJitter {
        record Lay(long ts, byte[] payload) {}

        int doSau;
        final int toiDa;
        final TreeMap<Long, Lay> khung = new TreeMap<>();     // số thứ tự mở rộng -> (ts, payload)
        long keTiep;
        boolean dangPhat;
        Long seqLonNhat, seqDau;
        long tsCuoi;
        int nhan, tre, thieu, can, tran;

        BoDemJitter(int doSau) { this(doSau, 60); }
        BoDemJitter(int doSau, int toiDa) { this.doSau = doSau; this.toiDa = toiDa; }

        /** Số thứ tự 16 bit quay vòng -> số nguyên tăng mãi. */
        private long moRong(int seq) {
            if (seqLonNhat == null) return seq;
            long lech = (seq - seqLonNhat) & 0xFFFF;
            return lech < 0x8000 ? seqLonNhat + lech : seqLonNhat - (0x10000 - lech);
        }

        void nhanGoi(int seq, long ts, byte[] payload) {
            long s = moRong(seq);
            if (seqDau == null) seqDau = s;
            if (seqLonNhat == null || s > seqLonNhat) seqLonNhat = s;
            nhan++;
            if (dangPhat && s < keTiep) {
                tre++;                                     // đến sau lượt phát của nó
                return;
            }
            khung.put(s, new Lay(ts, payload));
            if (khung.size() > toiDa) {
                khung.pollFirstEntry();
                tran++;
            }
        }

        /** Gọi mỗi 20 ms. Trả về (ts, payload|null) hoặc null (đang chờ nạp). */
        Lay lay() {
            if (!dangPhat) {
                if (khung.size() < doSau) return null;
                dangPhat = true;
                keTiep = khung.firstKey();
            }
            Lay k = khung.remove(keTiep);
            if (k != null) {
                keTiep++;
                tsCuoi = k.ts();
                return k;
            }
            if (khung.isEmpty()) {
                dangPhat = false;
                can++;
                return null;
            }
            thieu++;                                       // mất khung này
            keTiep++;
            tsCuoi = (tsCuoi + MAU_MOI_KHUNG) & 0xFFFFFFFFL;
            return new Lay(tsCuoi, null);
        }

        /** Bỏ mọi khung đang giữ (khi tắt nhận âm thanh); lần sau nạp lại từ đầu. */
        void xoa() {
            khung.clear();
            dangPhat = false;
        }

        /** Đổi độ sâu khi đang chạy: tăng -> dừng phát để nạp thêm; giảm -> bỏ khung cũ nhất. */
        void datDoSau(int n) {
            doSau = n;
            if (khung.size() < n) dangPhat = false;
            while (khung.size() > n) {
                long s = khung.pollFirstEntry().getKey();
                if (dangPhat) keTiep = s + 1;
            }
        }

        /** Số gói mất theo RFC 3550: số gói mong đợi trừ số gói đã nhận. */
        long mat() {
            if (seqDau == null) return 0;
            return Math.max(0, seqLonNhat - seqDau + 1 - nhan);
        }
    }

    // ================================================================ Gói video
    /** Chia dữ liệu một khung (I hoặc P) thành datagram ≤ 1200 byte.
     *  Header: phiên bản 1, loại, mã nguồn, số khung, chỉ số mảnh, tổng số mảnh, rộng, cao, nhãn ms. */
    static List<byte[]> chiaManh(int loai, int ma, long soKhung, int rong, int cao, long tsMs, byte[] duLieu) {
        int tong = Math.max(1, (duLieu.length + DU_LIEU_TOI_DA - 1) / DU_LIEU_TOI_DA);
        if (tong > MANH_TOI_DA) throw new IllegalArgumentException("khung quá lớn");
        List<byte[]> ds = new ArrayList<>(tong);
        for (int i = 0; i < tong; i++) {
            int dau = i * DU_LIEU_TOI_DA, dai = Math.min(duLieu.length, dau + DU_LIEU_TOI_DA) - dau;
            ByteBuffer b = ByteBuffer.allocate(CO_HEADER + dai);
            b.put((byte) PHIEN_BAN).put((byte) loai).putShort((short) ma).putInt((int) soKhung).putShort((short) i)
             .putShort((short) tong).putShort((short) rong).putShort((short) cao).putInt((int) tsMs).put(duLieu, dau, dai);
            ds.add(b.array());
        }
        return ds;
    }

    /** Gói giữ kết nối / đăng ký địa chỉ UDP: loại 2, các trường khác 0. */
    static byte[] goiGiu(int ma) {
        ByteBuffer b = ByteBuffer.allocate(CO_HEADER);
        b.put((byte) PHIEN_BAN).put((byte) LOAI_GIU).putShort((short) ma);
        return b.array();
    }

    record Header(int loai, int ma, long so, int i, int tong, int w, int h, long ts, byte[] payload) {}

    /** Tách header; null nếu sai dạng. */
    static Header tachHeader(byte[] g, int len) {
        if (len < CO_HEADER || len > MAX_DATAGRAM || g[0] != PHIEN_BAN) return null;
        ByteBuffer b = ByteBuffer.wrap(g, 0, len);
        int loai = g[1] & 0xFF;
        if (loai != LOAI_GIU && loai != LOAI_I && loai != LOAI_P) return null;
        int ma = b.getShort(2) & 0xFFFF, i = b.getShort(8) & 0xFFFF, tong = b.getShort(10) & 0xFFFF;
        int w = b.getShort(12) & 0xFFFF, h = b.getShort(14) & 0xFFFF;
        if (loai != LOAI_GIU && (tong == 0 || tong > MANH_TOI_DA || i >= tong || w == 0 || h == 0)) return null;
        byte[] p = new byte[len - CO_HEADER];
        System.arraycopy(g, CO_HEADER, p, 0, p.length);
        return new Header(loai, ma, b.getInt(4) & 0xFFFFFFFFL, i, tong, w, h, b.getInt(16) & 0xFFFFFFFFL, p);
    }

    /** Khung vừa đủ mảnh: (loại, số khung, ts, rộng, cao, dữ liệu nén). */
    record Khung(int loai, long so, long ts, int w, int h, byte[] duLieu) {}

    /**
     * Ghép mảnh thành khung (I hoặc P). Khác MR6.3: KHÔNG bỏ khung cũ còn dở khi khung mới đủ, vì khung P cần
     * mọi khung trước nó – khung dở chỉ bị bỏ khi chờ quá `cho` giây. Không an toàn luồng.
     */
    static final class GhepKhung {
        static final class Dang {
            final int loai, tong, w, h;
            final long ts;
            final double t;
            final Map<Integer, byte[]> manh = new HashMap<>();
            Dang(int loai, int tong, int w, int h, long ts, double t) {
                this.loai = loai; this.tong = tong; this.w = w; this.h = h; this.ts = ts; this.t = t;
            }
        }

        final double cho;
        final Map<Long, Dang> dang = new HashMap<>();     // số khung -> khung đang ghép
        Set<Long> xong = new HashSet<>();                // khung đã đủ hoặc đã bỏ (mảnh đến sau = trễ)
        long lonNhat = -1;
        int manhNhan, khungDu, khungHong, manhMat, tre;

        GhepKhung() { this(CHO_MANH_S); }
        GhepKhung(double cho) { this.cho = cho; }

        void donDep(double bayGio) { donDep(bayGio, null); }

        void donDep(double bayGio, Long tru) {
            for (Iterator<Map.Entry<Long, Dang>> it = dang.entrySet().iterator(); it.hasNext(); ) {
                Map.Entry<Long, Dang> e = it.next();
                if (bayGio - e.getValue().t > cho && !e.getKey().equals(tru)) {
                    it.remove();
                    khungHong++;
                    manhMat += e.getValue().tong - e.getValue().manh.size();
                    xong.add(e.getKey());
                }
            }
        }

        /** Bỏ các khung đang ghép dở mà không tính là hỏng (khi tắt nhận video). */
        void boDangDo() {
            xong.addAll(dang.keySet());
            dang.clear();
        }

        /** Trả về khung khi vừa đủ mảnh, ngược lại null. */
        Khung them(int loai, long so, int i, int tong, int w, int h, long ts, byte[] payload, double bayGio) {
            manhNhan++;
            if (xong.contains(so) || so < lonNhat - 1000) {
                tre++;
                return null;
            }
            Dang k = dang.computeIfAbsent(so, s -> new Dang(loai, tong, w, h, ts, bayGio));
            if (k.loai != loai || k.tong != tong || k.w != w || k.h != h || k.ts != ts) return null;   // mâu thuẫn: bỏ mảnh
            k.manh.put(i, payload);
            donDep(bayGio, so);
            if (k.manh.size() < tong) return null;
            dang.remove(so);
            xong.add(so);
            lonNhat = Math.max(lonNhat, so);
            if (xong.size() > 3000) {
                Set<Long> moi = new HashSet<>();
                for (long s : xong) if (s > lonNhat - 1000) moi.add(s);
                xong = moi;
            }
            khungDu++;
            ByteArrayOutputStream d = new ByteArrayOutputStream();
            for (int j = 0; j < tong; j++) d.writeBytes(k.manh.get(j));
            return new Khung(loai, so, ts, w, h, d.toByteArray());
        }
    }

    // ================================================================ Mã hóa / giải mã I, P
    /** XOR từng byte hai khung cùng kích thước. */
    static byte[] xorKhung(byte[] a, byte[] b) {
        byte[] r = new byte[a.length];
        for (int i = 0; i < a.length; i++) r[i] = (byte) (a[i] ^ b[i]);
        return r;
    }

    /** Nén zlib mức 6 (định dạng zlib có header 2 byte và Adler-32 – như zlib.compress(…, 6) của Python). */
    static byte[] nenZlib(byte[] b) {
        Deflater d = new Deflater(6);
        d.setInput(b);
        d.finish();
        ByteArrayOutputStream out = new ByteArrayOutputStream(b.length / 4 + 64);
        byte[] buf = new byte[16384];
        while (!d.finished()) out.write(buf, 0, d.deflate(buf));
        d.end();
        return out.toByteArray();
    }

    /** Giải nén zlib có giới hạn kích thước đầu ra 4 MB (chống "bom nén"); null nếu hỏng hoặc quá lớn. */
    static byte[] giaiNen(byte[] b) {
        Inflater inf = new Inflater();
        inf.setInput(b);
        ByteArrayOutputStream out = new ByteArrayOutputStream(RONG * CAO * 3);
        byte[] buf = new byte[16384];
        try {
            while (!inf.finished()) {
                int n = inf.inflate(buf);
                if (n == 0 && (inf.needsInput() || inf.needsDictionary())) break;   // dữ liệu cụt: trả phần đã có
                out.write(buf, 0, n);
                if (out.size() > (4 << 20)) return null;
            }
            return out.toByteArray();
        } catch (DataFormatException e) {
            return null;
        } finally {
            inf.end();
        }
    }

    static byte[] maHoaI(byte[] anh) { return nenZlib(anh); }

    static byte[] maHoaP(byte[] anh, byte[] truoc) { return nenZlib(xorKhung(anh, truoc)); }

    /** Khung vừa giải được: (số, loại, ts, rộng, cao, ảnh RGB). */
    record KhungGiai(long so, int loai, long ts, int w, int h, byte[] anh) {}

    /**
     * Giải mã chuỗi I/P theo thứ tự số khung.
     *  - I: giải nén -> thành khung tham chiếu (bỏ mọi khung cũ hơn đang chờ).
     *  - P số k: giải được khi tham chiếu là k−1. Nếu khung k đủ mảnh mà tham chiếu < k−1: giữ chờ khung thiếu tối đa
     *    CHO_THAM_CHIEU_S; quá hạn -> MẤT THAM CHIẾU: đếm đứng hình, bỏ mọi P tới I kế tiếp, kiemTra() trả true để
     *    người xem gửi xin_khung_khoa.
     *  - Chưa từng có tham chiếu (vừa vào xem): chỉ chờ I, không tính đứng hình.
     * Không an toàn luồng.
     */
    static final class GiaiMaVideo {
        record Doi(long ts, int w, int h, byte[] duLieu, double luc) {}

        final double cho;
        Long thamChieuSo;                 // tham chiếu: (số khung, ảnh, rộng, cao); null = chưa có
        byte[] thamChieuAnh;
        int thamChieuW, thamChieuH;
        boolean matThamChieu;
        Double lucMat;
        TreeMap<Long, Doi> doi = new TreeMap<>();       // số khung P -> (ts, w, h, dữ liệu, lúc đủ mảnh)
        int soI, soP, boP, dungHinh, loi;

        GiaiMaVideo() { this(CHO_THAM_CHIEU_S); }
        GiaiMaVideo(double cho) { this.cho = cho; }

        /** Trả về danh sách khung vừa giải được. */
        List<KhungGiai> them(Khung k, double bayGio) {
            List<KhungGiai> ra = new ArrayList<>();
            long so = k.so();
            if (k.loai() == LOAI_I) {
                if (thamChieuSo != null && so <= thamChieuSo) return ra;      // I cũ hơn tham chiếu hiện tại
                byte[] anh = giaiNen(k.duLieu());
                if (anh == null || anh.length != k.w() * k.h() * 3) {
                    loi++;
                    return ra;
                }
                thamChieuSo = so;
                thamChieuAnh = anh;
                thamChieuW = k.w();
                thamChieuH = k.h();
                matThamChieu = false;
                doi = new TreeMap<>(doi.tailMap(so, false));
                soI++;
                ra.add(new KhungGiai(so, LOAI_I, k.ts(), k.w(), k.h(), anh));
                giaiTiep(ra);
                return ra;
            }
            if (thamChieuSo == null || matThamChieu) {
                boP++;                                                       // chưa có / mất tham chiếu: chờ I
                return ra;
            }
            if (so <= thamChieuSo) return ra;
            doi.put(so, new Doi(k.ts(), k.w(), k.h(), k.duLieu(), bayGio));
            giaiTiep(ra);
            return ra;
        }

        private void giaiTiep(List<KhungGiai> ra) {
            while (thamChieuSo != null && doi.containsKey(thamChieuSo + 1)) {
                long so = thamChieuSo + 1;
                Doi d = doi.remove(so);
                byte[] x = giaiNen(d.duLieu());
                if (x == null || x.length != thamChieuAnh.length || d.w() != thamChieuW || d.h() != thamChieuH) {
                    loi++;                                                   // P hỏng: coi như thiếu khung này
                    return;
                }
                byte[] anh = xorKhung(x, thamChieuAnh);
                thamChieuSo = so;
                thamChieuAnh = anh;
                soP++;
                ra.add(new KhungGiai(so, LOAI_P, d.ts(), d.w(), d.h(), anh));
            }
        }

        /** true nếu vừa mất tham chiếu (khung thiếu không tới sau CHO_THAM_CHIEU_S). */
        boolean kiemTra(double bayGio) {
            if (doi.isEmpty()) return false;
            double som = Double.MAX_VALUE;
            for (Doi d : doi.values()) som = Math.min(som, d.luc());
            if (bayGio - som < cho) return false;
            matThamChieu = true;
            lucMat = bayGio;
            dungHinh++;
            boP += doi.size();
            doi.clear();
            return true;
        }
    }

    // ================================================================ Ảnh RGB, phông 5×7, PNG
    /** Phông chữ bitmap 5×7: mỗi ký tự 7 hàng, mỗi hàng 5 bit (bit cao = điểm bên trái), viết hex. */
    static final Map<Character, String> PHONG = new HashMap<>();
    static {
        String[] ds = {"A0E11111F111111", "B1E11111E11111E", "C0E11101010110E", "D1E11111111111E", "E1F10101E10101F",
            "F1F10101E101010", "G0E11101711110F", "H1111111F111111", "I0E04040404040E", "J0702020202120C", "K11121418141211",
            "L1010101010101F", "M111B1515111111", "N11111915131111", "O0E11111111110E", "P1E11111E101010", "Q0E11111115120D",
            "R1E11111E141211", "S0F10100E01011E", "T1F040404040404", "U1111111111110E", "V11111111110A04", "W1111111515150A",
            "X11110A040A1111", "Y1111110A040404", "Z1F01020408101F", "00E11131519110E", "1040C040404040E", "20E11010204081F",
            "31F02040201110E", "402060A121F0202", "51F101E0101110E", "60608101E11110E", "71F010204080808", "80E11110E11110E",
            "90E11110F01020C", "#0A0A1F0A1F0A0A", "/01010204081010", ":000C0C000C0C00", ";000C0C000C0408", ".00000000000C0C",
            "-0000001F000000", "(02040808080402", ")08040202020408", "=00001F001F0000", ",000000000C0408", "+0004041F040400",
            "?0E110102040004", " 00000000000000"};
        for (String s : ds) PHONG.put(s.charAt(0), s.substring(1));
    }

    static final int[] TRANG = {255, 255, 255}, DEN = {0, 0, 0};

    /** Tô hình chữ nhật (x, y, rw, rh) bằng màu {r, g, b} (cắt theo mép ảnh). */
    static void toMau(byte[] anh, int w, int x, int y, int rw, int rh, int[] m) {
        rw = Math.max(0, Math.min(rw, w - x));
        for (int yy = y; yy < y + rh; yy++) {
            int dau = (yy * w + x) * 3;
            if (dau + rw * 3 > anh.length) break;
            for (int k = 0; k < rw; k++) {
                anh[dau + 3 * k] = (byte) m[0];
                anh[dau + 3 * k + 1] = (byte) m[1];
                anh[dau + 3 * k + 2] = (byte) m[2];
            }
        }
    }

    /** Vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng co lần. */
    static void veChu(byte[] anh, int w, int h, int x, int y, String chu, int co, int[] m) {
        for (char c : chu.toUpperCase(Locale.ROOT).toCharArray()) {
            String ma = PHONG.getOrDefault(c, PHONG.get('?'));
            for (int hang = 0; hang < 7; hang++) {
                int bits = Integer.parseInt(ma.substring(2 * hang, 2 * hang + 2), 16);
                for (int cot = 0; cot < 5; cot++) {
                    int px = x + cot * co, py = y + hang * co;
                    if ((bits & (0x10 >> cot)) != 0 && px + co <= w && py + co <= h) toMau(anh, w, px, py, co, co, m);
                }
            }
            x += 6 * co;
        }
    }

    /** Phóng to k lần bằng lặp điểm gần nhất. */
    static byte[] phongTo(byte[] anh, int w, int h, int k) {
        if (k == 1) return anh.clone();
        byte[] ra = new byte[w * k * h * k * 3];
        for (int y = 0; y < h * k; y++)
            for (int x = 0; x < w * k; x++) System.arraycopy(anh, ((y / k) * w + x / k) * 3, ra, (y * w * k + x) * 3, 3);
        return ra;
    }

    /** Ghi ảnh RGB thô thành PNG bằng javax.imageio (PNG: chữ ký 8 byte, khối IHDR, IDAT – zlib, IEND; CRC-32). */
    static void ghiPng(String duongDan, byte[] anh, int w, int h) throws IOException {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_RGB);
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++) {
                int i = (y * w + x) * 3;
                img.setRGB(x, y, ((anh[i] & 0xFF) << 16) | ((anh[i + 1] & 0xFF) << 8) | (anh[i + 2] & 0xFF));
            }
        File tep = new File(duongDan);
        if (tep.getParentFile() != null) tep.getParentFile().mkdirs();
        ImageIO.write(img, "png", tep);
    }

    record AnhNhan(int w, int h, byte[] rgb, String nhan) {}

    /** Ghép các ảnh thành một PNG: `cot` cột, nhãn dưới mỗi ảnh. Trả về {rộng, cao}. */
    static int[] ghepAnh(String duongDan, List<AnhNhan> ds, int cot) throws IOException {
        int oW = 0;
        for (AnhNhan a : ds) oW = Math.max(oW, a.w());
        oW += 16;
        List<Integer> caoHang = new ArrayList<>();
        for (int i = 0; i < ds.size(); i += cot) {
            int m = 0;
            for (int j = i; j < Math.min(ds.size(), i + cot); j++) m = Math.max(m, ds.get(j).h());
            caoHang.add(m + 30);
        }
        int W = Math.min(cot, ds.size()) * oW + 16, H = 8;
        for (int c : caoHang) H += c;
        byte[] anh = new byte[W * H * 3];
        java.util.Arrays.fill(anh, (byte) 0xFF);
        for (int n = 0; n < ds.size(); n++) {
            AnhNhan a = ds.get(n);
            int x0 = 16 + (n % cot) * oW, y0 = 8;
            for (int r = 0; r < n / cot; r++) y0 += caoHang.get(r);
            for (int y = 0; y < a.h(); y++) System.arraycopy(a.rgb(), y * a.w() * 3, anh, ((y0 + y) * W + x0) * 3, a.w() * 3);
            String nhan = a.nhan().length() > oW / 12 ? a.nhan().substring(0, oW / 12) : a.nhan();
            veChu(anh, W, H, x0, y0 + a.h() + 8, nhan, 2, DEN);
        }
        ghiPng(duongDan, anh, W, H);
        return new int[] {W, H};
    }

    // ================================================================ Nguồn giả lập "bảng clapper"
    static final int[][] BANG_MAU = {{230, 120, 60}, {70, 170, 90}, {70, 120, 220}, {200, 80, 180}, {200, 190, 60}};

    /**
     * Camera giả lập kiểu bảng clapper của nhà quay phim: nền tĩnh (sóng chéo + hạt nhiễu thưa như MR6.3), một ô
     * vuông chạy ngang (chuyển động nhỏ), đồng hồ phương tiện T=12.345 và số khung. Ở mỗi giây tròn
     * (ts % 1000 < 1000/fps) cả khung CHỚP TRẮNG (giữ mờ vân nền) – cùng lúc tiếng bíp của NguonAmClapper: nhìn hình,
     * nghe tiếng là thấy lệch tiếng-hình.
     */
    static final class NguonClapper {
        final String ten;
        volatile int fps;
        final byte[] nen = new byte[RONG * CAO * 3], nenChop = new byte[RONG * CAO * 3];

        NguonClapper(String ten, int ma, int fps) {
            this.ten = ten;
            this.fps = fps;
            int[] mau = BANG_MAU[Math.floorMod(ma - 1, BANG_MAU.length)];
            Random r = new Random(ma);
            for (int y = 0; y < CAO; y++)
                for (int x = 0; x < RONG; x++) {
                    double k = 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(2 * Math.PI * (x / 40.0 + y / 60.0)));
                    int hat = r.nextDouble() < 0.04 ? r.nextInt(24) : 0;          // hạt nhiễu
                    int i = (y * RONG + x) * 3;
                    for (int c = 0; c < 3; c++) nen[i + c] = (byte) Math.min(255, (int) (mau[c] * k) / 16 * 16 + hat);
                }
            // Khung chớp gần như trắng nhưng còn vân nền: khung I rơi vào lúc chớp vẫn có kích thước như khung I
            // thường. Bảng tra 256 giá trị: v -> 255 − (255 − v)/4.
            for (int i = 0; i < nen.length; i++) {
                int v = nen[i] & 0xFF;
                nenChop[i] = (byte) (255 - (255 - v) / 4);
            }
        }

        byte[] layKhung(long so, long tsMs) {
            boolean chop = tsMs % 1000 < 1000.0 / fps;
            int x = (int) (6 + (tsMs / 10) % (RONG - 24));                    // ô vuông chạy 100 điểm ảnh/giây
            byte[] anh;
            int[] mauChu, nenChu;
            if (chop) {                                                        // chớp trắng
                anh = nenChop.clone();
                mauChu = DEN;
                nenChu = TRANG;
            } else {
                anh = nen.clone();
                mauChu = TRANG;
                nenChu = new int[] {20, 20, 20};
                toMau(anh, RONG, x, 52, 12, 12, new int[] {255, 230, 0});
            }
            String dongHo = f("T=%d.%03d", tsMs / 1000, tsMs % 1000);
            toMau(anh, RONG, 4, 4, 6 * 2 * dongHo.length() + 4, 18, nenChu);
            veChu(anh, RONG, CAO, 6, 6, dongHo, 2, mauChu);
            String nhan = f("#%04d", so);
            toMau(anh, RONG, 4, CAO - 22, 6 * 2 * nhan.length() + 4, 18, nenChu);
            veChu(anh, RONG, CAO, 6, CAO - 20, nhan, 2, mauChu);
            toMau(anh, RONG, RONG - 30, CAO - 15, 27, 11, new int[] {200, 0, 0});
            veChu(anh, RONG, CAO, RONG - 28, CAO - 13, "LIVE", 1, TRANG);
            String t3 = ten.length() > 3 ? ten.substring(0, 3) : ten;
            veChu(anh, RONG, CAO, RONG - 6 * 2 * t3.length() - 4, 6, t3, 2, mauChu);
            return anh;
        }
    }

    /**
     * Micro giả lập: âm 440 Hz biên độ thấp liên tục + tiếng "bíp" 1000 Hz dài 100 ms ở mỗi giây tròn của đồng hồ
     * phương tiện (mẫu thứ n: n % 16000 < 1600). Mẫu đầu tiên = lúc bấm Phát.
     */
    static class NguonAmClapper {
        long n;
        final double w, a, wb, ab;

        NguonAmClapper() { this(440.0, 0.08, 1000.0, 0.4); }

        NguonAmClapper(double tanSo, double bienDo, double tanSoBip, double bienDoBip) {
            w = 2 * Math.PI * tanSo / TAN_SO_MAU;
            a = bienDo * 32767;
            wb = 2 * Math.PI * tanSoBip / TAN_SO_MAU;
            ab = bienDoBip * 32767;
        }

        public short[] layKhung() {
            short[] ra = new short[MAU_MOI_KHUNG];
            for (int k = 0; k < MAU_MOI_KHUNG; k++) {
                long m = n + k;
                double x = a * Math.sin(w * m);
                if (m % TAN_SO_MAU < TAN_SO_MAU / 10) x += ab * Math.sin(wb * m);
                ra[k] = (short) Math.round(x);
            }
            n += MAU_MOI_KHUNG;
            return ra;
        }
    }

    // ================================================================ Kênh điều khiển JSON theo dòng
    /** Mỗi thông điệp là một dòng JSON UTF-8 kết thúc '\n' (người gọi giữ khóa gửi của socket). */
    static void guiJson(OutputStream out, Map<String, Object> obj) throws IOException {
        out.write((Json.stringify(obj) + "\n").getBytes(StandardCharsets.UTF_8));
        out.flush();
    }

    /**
     * Đọc một dòng JSON từ luồng vào (đã bọc BufferedInputStream). Trả về Map, hoặc null khi kết nối đóng;
     * dòng > 64 KB, bị cắt hoặc sai dạng -> IOException.
     */
    @SuppressWarnings("unchecked")
    static Map<String, Object> docDong(InputStream in) throws IOException {
        ByteArrayOutputStream b = new ByteArrayOutputStream(256);
        int c;
        while ((c = in.read()) != -1) {
            if (c == '\n') break;
            b.write(c);
            if (b.size() > GIOI_HAN_DONG) throw new IOException("dòng điều khiển quá dài hoặc bị cắt");
        }
        if (c == -1) {
            if (b.size() == 0) return null;
            throw new IOException("dòng điều khiển quá dài hoặc bị cắt");
        }
        Object o;
        try {
            o = Json.parse(b.toString(StandardCharsets.UTF_8));
        } catch (RuntimeException e) {
            throw new IOException("JSON sai: " + e.getMessage());
        }
        if (!(o instanceof Map<?, ?>)) throw new IOException("thông điệp không phải đối tượng JSON");
        return (Map<String, Object>) o;
    }

    static boolean tenHopLe(Object o) {
        if (!(o instanceof String ten)) return false;
        int n = ten.codePointCount(0, ten.length());
        if (n < 1 || n > 16 || !ten.strip().equals(ten)) return false;
        return ten.codePoints().allMatch(cp -> cp == ' ' || (!Character.isISOControl(cp) && !Character.isWhitespace(cp)
                && Character.getType(cp) != Character.FORMAT && Character.getType(cp) != Character.UNASSIGNED));
    }

    /** Lấy chuỗi để in: giống str() của Python cho số nguyên (10 chứ không phải 10.0). */
    static String chu(Object v) {
        if (v instanceof Double d && d == Math.rint(d) && !d.isInfinite()) return String.valueOf(d.longValue());
        return String.valueOf(v == null ? "None" : v);
    }

    static double so(Object v, double macDinh) { return v instanceof Number n ? n.doubleValue() : macDinh; }

    static boolean dung(Object v, boolean macDinh) {
        if (v instanceof Boolean b) return b;
        if (v instanceof Number n) return n.doubleValue() != 0;
        if (v instanceof String s) return !s.isEmpty();
        return v == null ? macDinh : true;
    }

    /** Đọc lệnh từ stdin; dòng "@t lệnh" = chờ tới giây thứ t kể từ t0 (đồng hồ đơn điệu). */
    static void docLenhStdin(double t0, Consumer<String> xuLy, Runnable ketThuc) {
        try (BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                String dong = line.strip();
                if (dong.startsWith("@")) {
                    String[] p = dong.split(" ", 2);
                    ngu(Double.parseDouble(p[0].substring(1)) - (bayGio() - t0));
                    dong = p.length > 1 ? p[1].strip() : "";
                }
                if (!dong.isEmpty()) xuLy.accept(dong);
            }
        } catch (IOException | NumberFormatException ignored) { }
        if (ketThuc != null) ketThuc.run();
    }

    static void chayNen(String ten, Runnable r) { Thread.ofPlatform().daemon().name(ten).start(r); }
}
