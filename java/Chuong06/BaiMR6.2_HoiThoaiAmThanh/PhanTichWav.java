import java.io.File;
import java.io.IOException;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.List;
import java.util.Locale;
import java.util.Map;

/**
 * Bài mở rộng MR6.2 – Phân tích bản ghi WAV: đo mức từng âm (Goertzel) trong từng đoạn.
 *
 * Đoạn được chia theo các mốc sự kiện client đã lưu (<tên>_moc.json); bỏ 0,15 s đầu mỗi đoạn vì âm thanh tới
 * tai người nghe trễ hơn thông báo TCP (độ sâu bộ đệm jitter + một nhịp trộn). Không có tệp mốc: chia mỗi 1 s.
 * Cuối cùng kiểm tra mix-minus: người ghi không được nghe tiếng của chính mình.
 *
 * Chạy: java -cp out PhanTichWav <tệp.wav> <tần_số:nhãn> ...
 */
public class PhanTichWav {
    record Am(double f, String nhan) {}
    record Moc(double t, List<String> ds) {}

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws IOException {
        final double boDau = 0.15, nganNhat = 0.3, nguong = -50.0;
        final double vang = Double.NEGATIVE_INFINITY;
        String wav = args[0];
        List<Am> ams = new ArrayList<>();
        for (int i = 1; i < args.length; i++) {
            String[] p = args[i].split(":", 2);
            ams.add(new Am(Double.parseDouble(p[0]), p[1]));
        }
        short[] mau;
        try {
            mau = AmThanh.docWAV(new File(wav));
        } catch (IOException e) {
            System.out.println("[phan_tich] Lỗi: " + e.getMessage());
            System.exit(1);
            return;
        }
        double dai = (double) mau.length / AmThanh.TAN_SO_MAU;
        System.out.printf(Locale.ROOT, "[phan_tich] %s: %.2f s, 16000 Hz, 16 bit, mono%n", wav, dai);
        System.out.printf("[phan_tich] Goertzel, cửa sổ Hann; RMS dBFS; --- = vắng (<%s)%n", KhachAmThanh.fmtG(nguong));

        // ---- Mốc chia đoạn
        List<double[]> tMoc = new ArrayList<>();
        List<String> tenMoc = new ArrayList<>();
        String tenMinh = "";
        File fMoc = new File(wav.replaceAll("\\.[^./]*$", "") + "_moc.json");
        if (fMoc.exists()) {
            Map<String, Object> d = (Map<String, Object>) Json.parse(Files.readString(fMoc.toPath(), StandardCharsets.UTF_8));
            tenMinh = String.valueOf(d.get("ten"));
            tMoc.add(new double[] {0});
            tenMoc.add("bắt đầu ghi");
            for (Object o : (List<Object>) d.get("moc")) {
                Map<String, Object> m = (Map<String, Object>) o;
                tMoc.add(new double[] {((Number) m.get("t")).doubleValue()});
                tenMoc.add(String.valueOf(m.get("su_kien")));
            }
        } else {
            for (int i = 0; i < (int) dai; i++) {
                tMoc.add(new double[] {i});
                tenMoc.add("cửa sổ " + (i + 1));
            }
        }
        List<Moc> nhom = new ArrayList<>();                       // gộp các mốc quá gần nhau thành một
        for (int i = 0; i < tMoc.size(); i++) {
            double t = tMoc.get(i)[0];
            if (!nhom.isEmpty() && t - nhom.get(nhom.size() - 1).t() < boDau + nganNhat) nhom.get(nhom.size() - 1).ds().add(tenMoc.get(i));
            else nhom.add(new Moc(t, new ArrayList<>(List.of(tenMoc.get(i)))));
        }

        // ---- Đo mức từng âm trong từng đoạn
        StringBuilder tieuDe = new StringBuilder("Đoạn  Từ-đến (s) ");
        for (Am a : ams) tieuDe.append(String.format("%2s %-4s", a.nhan(), KhachAmThanh.fmtG(a.f())));
        System.out.println(tieuDe + " Thay đổi");
        double[] truoc = null;
        List<double[]> ketQua = new ArrayList<>();
        for (int i = 0; i < nhom.size(); i++) {
            double dau = nhom.get(i).t() + boDau;
            double cuoi = i + 1 < nhom.size() ? nhom.get(i + 1).t() : dai;
            if (cuoi - dau < 0.1) continue;
            int from = (int) (dau * AmThanh.TAN_SO_MAU), to = (int) (cuoi * AmThanh.TAN_SO_MAU);
            double[] muc = new double[ams.size()];
            StringBuilder dong = new StringBuilder(String.format(Locale.ROOT, "%3d   %4.2f-%4.2f ", i + 1, dau, cuoi));
            for (int j = 0; j < ams.size(); j++) {
                muc[j] = AmThanh.mucAmDBFS(mau, from, to, ams.get(j).f());
                if (muc[j] < nguong) muc[j] = vang;
                dong.append(AmThanh.chuDB(muc[j], 7));
            }
            List<String> doi = new ArrayList<>();
            if (truoc != null) {
                for (int j = 0; j < ams.size(); j++) {
                    double m0 = truoc[j], m1 = muc[j];
                    String n = ams.get(j).nhan();
                    if (m0 == vang && m1 > vang) doi.add(n + " bật");
                    else if (m0 > vang && m1 == vang) doi.add(n + " tắt");
                    else if (m0 > vang && Math.abs(m1 - m0) >= 1) doi.add(String.format(Locale.ROOT, "%s %+.1fdB", n, m1 - m0));
                }
            }
            System.out.println(dong + "  " + String.join(", ", doi));
            truoc = muc;
            ketQua.add(muc);
        }
        System.out.println("Sự kiện mở đầu mỗi đoạn:");
        for (int i = 0; i < nhom.size(); i++)
            System.out.printf(Locale.ROOT, "%3d %5.2fs %s%n", i + 1, nhom.get(i).t(), String.join("; ", nhom.get(i).ds()));

        // ---- Kiểm tra mix-minus: người nghe không được nghe tiếng của chính mình
        for (int j = 0; j < ams.size(); j++) {
            Am a = ams.get(j);
            if (!a.nhan().equals(tenMinh)) continue;
            double lonNhat = vang;
            for (double[] m : ketQua) lonNhat = Math.max(lonNhat, m[j]);
            String kq = lonNhat > vang ? String.format(Locale.ROOT, "%.1f dBFS -> SAI", lonNhat) : "vắng mọi đoạn -> ĐÚNG";
            System.out.printf("Mix-minus: %s không nghe tiếng mình (%s Hz): %s%n", a.nhan(), KhachAmThanh.fmtG(a.f()), kq);
        }
    }
}
