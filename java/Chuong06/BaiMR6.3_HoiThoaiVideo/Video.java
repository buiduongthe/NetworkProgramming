import java.awt.image.BufferedImage;
import java.io.BufferedReader;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.net.Socket;
import java.nio.ByteBuffer;
import java.nio.charset.StandardCharsets;
import java.util.HashMap;
import java.util.Iterator;
import java.util.Map;
import java.util.Random;
import java.util.function.Predicate;
import java.util.zip.DataFormatException;
import java.util.zip.Deflater;
import java.util.zip.Inflater;
import javax.imageio.ImageIO;

/**
 * Các thủ tục dùng chung cho server và client hội thoại video.
 *
 * Khung hình là ma trận điểm ảnh; mỗi điểm RGB 3 byte. Khung 160×120:
 *  - kích thước thô: 160 × 120 × 3 = 57 600 byte; ở 10 khung/giây (fps) cần 57 600 × 8 × 10 ≈ 4,6 Mbit/s –
 *    quá lớn, nên phải nén trước khi gửi;
 *  - nén bằng zlib (không mất dữ liệu; java.util.zip.Deflater); khung tổng hợp có "hạt nhiễu" giống cảm biến
 *    camera nên nén còn khoảng 7–9 KB;
 *  - giảm độ phân giải 1/2 mỗi chiều -> số điểm ảnh còn 1/4, dung lượng giảm tương ứng.
 * Khung nén lớn hơn một gói UDP an toàn, nên được chia mảnh: mỗi datagram ≤ 1200 byte (mức QUIC – RFC 9000 –
 * coi là an toàn), gồm header 20 byte + tối đa 1180 byte dữ liệu. Bên nhận ghép đủ mảnh mới giải nén được;
 * mất một mảnh là hỏng cả khung. Định dạng gói, JSON điều khiển giống hệt bản Python, Go.
 */
final class Video {
    private Video() {}

    static final int RONG = 160, CAO = 120;            // độ phân giải gốc của camera
    static final int MAX_DATAGRAM = 1200, CO_HEADER = 20, DU_LIEU_TOI_DA = MAX_DATAGRAM - CO_HEADER;   // 1180 byte
    static final int PHIEN_BAN = 1, LOAI_MANH = 1, LOAI_GIU = 2, MA_SERVER = 0;
    static final int FPS_MAC_DINH = 10;
    static final int MANH_TOI_DA = 256;                // tối đa 256 mảnh/khung (≈ 300 KB): chặn khung giả quá lớn
    static final long T0 = System.nanoTime();

    static double giay() { return (System.nanoTime() - T0) / 1e9; }

    // ---------------------------------------------------------------- Chia mảnh và ghép khung

    /** Header: phiên bản(1) loại(1) mã nguồn(2) số khung(4) chỉ số mảnh(2) tổng mảnh(2) rộng(2) cao(2) ts ms(4). */
    record Header(int loai, int ma, long soKhung, int chiSo, int tong, int w, int h) {
        byte[] dongGoi(byte[] payload, int off, int len) {
            ByteBuffer b = ByteBuffer.allocate(CO_HEADER + len);   // big-endian
            b.put((byte) PHIEN_BAN).put((byte) loai).putShort((short) ma).putInt((int) soKhung)
             .putShort((short) chiSo).putShort((short) tong).putShort((short) w).putShort((short) h)
             .putInt((int) ((System.nanoTime() - T0) / 1_000_000)).put(payload, off, len);
            return b.array();
        }
    }

    /** Chia dữ liệu khung nén thành các datagram ≤ 1200 byte. */
    static byte[][] chiaManh(int ma, long soKhung, int w, int h, byte[] duLieu) {
        int tong = Math.max(1, (duLieu.length + DU_LIEU_TOI_DA - 1) / DU_LIEU_TOI_DA);
        byte[][] ds = new byte[tong][];
        for (int i = 0; i < tong; i++) {
            int off = i * DU_LIEU_TOI_DA, len = Math.min(duLieu.length, off + DU_LIEU_TOI_DA) - off;
            ds[i] = new Header(LOAI_MANH, ma, soKhung, i, tong, w, h).dongGoi(duLieu, off, len);
        }
        return ds;
    }

    static byte[] goiGiu(int ma) { return new Header(LOAI_GIU, ma, 0, 0, 0, 0, 0).dongGoi(new byte[0], 0, 0); }

    record Manh(Header h, byte[] payload) {}

    /** Tách header; null nếu không hợp lệ. */
    static Manh tachHeader(byte[] b, int len) {
        if (len < CO_HEADER || b[0] != PHIEN_BAN) return null;
        ByteBuffer bb = ByteBuffer.wrap(b, 0, len);
        Header h = new Header(b[1] & 0xFF, bb.getShort(2) & 0xFFFF, bb.getInt(4) & 0xFFFFFFFFL, bb.getShort(8) & 0xFFFF,
                bb.getShort(10) & 0xFFFF, bb.getShort(12) & 0xFFFF, bb.getShort(14) & 0xFFFF);
        if (h.loai() == LOAI_MANH && (h.tong() == 0 || h.tong() > MANH_TOI_DA || h.chiSo() >= h.tong())) return null;
        byte[] p = new byte[len - CO_HEADER];
        System.arraycopy(b, CO_HEADER, p, 0, p.length);
        return new Manh(h, p);
    }

    static final class KhungDo {
        final int tong;
        final Map<Integer, byte[]> manh = new HashMap<>();
        final long t;
        KhungDo(int tong, long t) { this.tong = tong; this.t = t; }
    }

    /**
     * Ghép các mảnh thành khung cho một luồng. Khi một khung đủ mảnh: trả về khung đó và bỏ mọi khung cũ hơn còn
     * dở (video trực tiếp chỉ cần khung mới nhất); khung dở quá 0,5 s cũng bị bỏ. Không an toàn luồng.
     */
    static final class GhepKhung {
        final Map<Long, KhungDo> dang = new HashMap<>();
        long cuoi = -1;                                // số khung đã xong/bỏ gần nhất (-1: chưa có)
        int manhNhan, khungDu, khungHong, manhMat, khungMatHan, tre;

        private void bo(Iterator<Map.Entry<Long, KhungDo>> it, KhungDo k) {
            it.remove();
            khungHong++;
            manhMat += k.tong - k.manh.size();
        }

        /** Thêm một mảnh; trả về dữ liệu khung nén khi đủ mảnh, ngược lại null. */
        byte[] them(Header h, byte[] payload, long bayGio) {
            manhNhan++;
            if (h.soKhung() <= cuoi) {
                tre++;                                 // thuộc khung đã xong hoặc đã bỏ
                return null;
            }
            KhungDo k = dang.computeIfAbsent(h.soKhung(), s -> new KhungDo(h.tong(), bayGio));
            k.manh.put(h.chiSo(), payload);
            for (Iterator<Map.Entry<Long, KhungDo>> it = dang.entrySet().iterator(); it.hasNext(); ) {
                Map.Entry<Long, KhungDo> e = it.next();
                if (e.getKey() != h.soKhung() && bayGio - e.getValue().t > 500_000_000L) bo(it, e.getValue());   // chờ quá lâu
            }
            if (k.manh.size() < k.tong) return null;
            ByteArrayOutputStream duLieu = new ByteArrayOutputStream();
            for (int j = 0; j < k.tong; j++) duLieu.writeBytes(k.manh.get(j));
            dang.remove(h.soKhung());
            int cu = 0;
            for (Iterator<Map.Entry<Long, KhungDo>> it = dang.entrySet().iterator(); it.hasNext(); ) {
                Map.Entry<Long, KhungDo> e = it.next();
                if (e.getKey() < h.soKhung()) {        // khung cũ còn dở: bỏ
                    bo(it, e.getValue());
                    cu++;
                }
            }
            if (cuoi >= 0) khungMatHan += Math.max(0, (int) (h.soKhung() - cuoi - 1) - cu);
            cuoi = h.soKhung();
            khungDu++;
            return duLieu.toByteArray();
        }
    }

    // ---------------------------------------------------------------- Ảnh RGB thô (byte[])

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
            "-0000001F000000", "(02040808080402", ")08040202020408", "=00001F001F0000", ",000000000C0408", "?0E110102040004",
            " 00000000000000"};
        for (String s : ds) PHONG.put(s.charAt(0), s.substring(1));
    }

    /** Tô hình chữ nhật (x, y, rw, rh) bằng một màu {r, g, b}. */
    static void toMau(byte[] anh, int w, int x, int y, int rw, int rh, int[] m) {
        for (int yy = y; yy < y + rh; yy++)
            for (int xx = x; xx < x + rw; xx++) {
                int i = (yy * w + xx) * 3;
                anh[i] = (byte) m[0];
                anh[i + 1] = (byte) m[1];
                anh[i + 2] = (byte) m[2];
            }
    }

    /** Vẽ chuỗi (chữ in hoa không dấu, số, vài ký hiệu) bằng phông 5×7 phóng co lần. */
    static void veChu(byte[] anh, int w, int h, int x, int y, String chu, int co, int[] m) {
        for (char c : chu.toUpperCase(java.util.Locale.ROOT).toCharArray()) {
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

    record Anh(byte[] rgb, int w, int h) {}

    /** Giảm độ phân giải d lần mỗi chiều: lấy 1 điểm trong mỗi khối d×d. */
    static Anh thuNho(byte[] anh, int w, int h, int d) {
        if (d == 1) return new Anh(anh, w, h);
        int w2 = w / d, h2 = h / d;
        byte[] ra = new byte[w2 * h2 * 3];
        for (int y = 0; y < h2; y++)
            for (int x = 0; x < w2; x++) System.arraycopy(anh, (y * d * w + x * d) * 3, ra, (y * w2 + x) * 3, 3);
        return new Anh(ra, w2, h2);
    }

    /** Phóng to k lần bằng lặp điểm gần nhất: ảnh độ phân giải thấp thành các ô vuông. */
    static byte[] phongTo(byte[] anh, int w, int h, int k) {
        byte[] ra = new byte[w * k * h * k * 3];
        for (int y = 0; y < h * k; y++)
            for (int x = 0; x < w * k; x++) System.arraycopy(anh, ((y / k) * w + x / k) * 3, ra, (y * w * k + x) * 3, 3);
        return ra;
    }

    // ---------------------------------------------------------------- Camera tổng hợp

    static final int[][] BANG_MAU = {{230, 120, 60}, {70, 170, 90}, {70, 120, 220}, {200, 80, 180}, {200, 190, 60}};

    /** Nền sóng chéo màu riêng của từng client, có hạt nhiễu thưa như cảm biến thật, trượt ngang theo thời gian. */
    static final class CameraTongHop {
        final String ten;
        final byte[] nen = new byte[RONG * CAO * 3];

        CameraTongHop(String ten, int ma) {
            this.ten = ten;
            int[] m = BANG_MAU[(ma - 1) % BANG_MAU.length];
            Random r = new Random(ma);
            for (int y = 0; y < CAO; y++)
                for (int x = 0; x < RONG; x++) {
                    double k = 0.35 + 0.65 * (0.5 + 0.5 * Math.sin(2 * Math.PI * (x / 40.0 + y / 60.0)));
                    int hat = r.nextDouble() < 0.04 ? r.nextInt(24) : 0;   // hạt nhiễu
                    for (int c = 0; c < 3; c++) nen[(y * RONG + x) * 3 + c] = (byte) Math.min(255, (int) (m[c] * k) / 16 * 16 + hat);
                }
        }

        byte[] layKhung(int so) {
            int dich = (so * 2) % RONG * 3;                // trượt 2 điểm ảnh mỗi khung
            byte[] anh = new byte[nen.length];
            for (int y = 0; y < CAO; y++) {
                int dau = y * RONG * 3, dai = RONG * 3;
                System.arraycopy(nen, dau + dich, anh, dau, dai - dich);
                System.arraycopy(nen, dau, anh, dau + dai - dich, dich);
            }
            int[] toi = {20, 20, 20}, trang = {255, 255, 255};
            toMau(anh, RONG, 6, 6, 6 * 4 * ten.length() + 6, 7 * 4 + 6, toi);
            veChu(anh, RONG, CAO, 9, 9, ten, 4, trang);
            String nhan = String.format("#%04d", so);
            toMau(anh, RONG, 6, CAO - 34, 6 * 2 * nhan.length() + 4, 7 * 2 + 4, toi);
            veChu(anh, RONG, CAO, 8, CAO - 32, nhan, 2, trang);
            return anh;
        }
    }

    // ---------------------------------------------------------------- Nén, PNG

    static byte[] nenZlib(byte[] b) {
        Deflater d = new Deflater(Deflater.BEST_SPEED);       // định dạng zlib (có header) như Python, Go
        d.setInput(b);
        d.finish();
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[16384];
        while (!d.finished()) out.write(buf, 0, d.deflate(buf));
        d.end();
        return out.toByteArray();
    }

    /** Giải nén có giới hạn kích thước (4 MB) chống "bom nén"; null nếu dữ liệu hỏng hoặc quá lớn. */
    static byte[] giaiNen(byte[] b) {
        Inflater inf = new Inflater();
        inf.setInput(b);
        ByteArrayOutputStream out = new ByteArrayOutputStream();
        byte[] buf = new byte[16384];
        try {
            while (!inf.finished()) {
                int n = inf.inflate(buf);
                if (n == 0 && (inf.needsInput() || inf.needsDictionary())) return null;   // dữ liệu cụt
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

    /** Ghi ảnh RGB thô thành PNG bằng javax.imageio. */
    static void ghiPNG(String duongDan, byte[] anh, int w, int h) throws IOException {
        BufferedImage img = new BufferedImage(w, h, BufferedImage.TYPE_INT_RGB);
        for (int y = 0; y < h; y++)
            for (int x = 0; x < w; x++) {
                int i = (y * w + x) * 3;
                img.setRGB(x, y, ((anh[i] & 0xFF) << 16) | ((anh[i + 1] & 0xFF) << 8) | (anh[i + 2] & 0xFF));
            }
        ImageIO.write(img, "png", new File(duongDan));
    }

    // ---------------------------------------------------------------- Kênh điều khiển JSON

    static void guiJSON(Socket s, Map<String, Object> v) {
        try {
            OutputStream out = s.getOutputStream();
            synchronized (s) { out.write((Json.stringify(v) + "\n").getBytes(StandardCharsets.UTF_8)); }
        } catch (IOException ignored) { }
    }

    @SuppressWarnings("unchecked")
    static Map<String, Object> docJSON(String line) {
        try {
            Object o = Json.parse(line);
            return o instanceof Map<?, ?> ? (Map<String, Object>) o : null;
        } catch (RuntimeException e) {
            return null;
        }
    }

    static int soNguyen(Object v, int macDinh) { return v instanceof Number n ? n.intValue() : macDinh; }

    /** Đọc lệnh từ stdin; "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy; phần sau # là chú thích. */
    static boolean chayKichBan(Predicate<String> lenh) {
        try (BufferedReader in = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8))) {
            String line;
            while ((line = in.readLine()) != null) {
                String dong = line.split("#", 2)[0].strip();
                if (dong.startsWith("@")) {
                    String[] p = dong.split(" ", 2);
                    long ms = (long) ((Double.parseDouble(p[0].substring(1)) - giay()) * 1000);
                    if (ms > 0) Thread.sleep(ms);
                    dong = p.length > 1 ? p[1].strip() : "";
                }
                if (!dong.isEmpty() && !lenh.test(dong)) return true;
            }
        } catch (IOException | InterruptedException ignored) { }
        return false;
    }
}
