import java.io.IOException;
import java.util.ArrayList;
import java.util.Collections;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.concurrent.BrokenBarrierException;
import java.util.concurrent.CyclicBarrier;
import java.util.concurrent.ThreadLocalRandom;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.TimeoutException;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.7 – Bộ mô phỏng tải: n người dùng ảo trên k kết nối, rào xuất phát, pipelining.
 *
 * Phần KHUNG (từ "KHUNG" trở xuống) giống nhau ở các bài tranh chấp; mỗi bài chỉ sửa phần NGHIỆP VỤ:
 *   - NguCanh: điều một kết nối biết về trạng thái chung (ở đây: số xe, số xe còn trống), cập nhật từ cap_nhat
 *     và từ trả lời kq;
 *   - NguoiAo: kịch bản của MỘT người dùng ảo: yeuCauDau() -> yêu cầu đầu tiên; sauKq(tin) -> yêu cầu kế tiếp
 *     hoặc null (xong). Yêu cầu là Map {"loai": ..., các trường...} (khung tự thêm ma_yc).
 *   - thamSoDatLai(): tham số gửi kèm dat_lai trước mỗi chế độ trong soSanh ({} = giữ tham số hiện tại của máy
 *     chủ, ví dụ số xe đã đặt bằng "Khach … dat_lai so_xe=10").
 *
 * Một lượt: mở k socket (người i đi trên kết nối i mod k; kết nối chào kèm "so_nguoi"), mọi luồng đứng chờ ở RÀO
 * XUẤT PHÁT (CyclicBarrier) -> cùng lúc mỗi kết nối gửi yêu cầu đầu của mọi người mình chở trong MỘT lần ghi;
 * trả lời tới đâu gửi yêu cầu kế tiếp của người đó tới đó. Đo thời gian tổng, yc/s, khứ hồi p50/p95/p99, rồi hỏi
 * thong_ke của máy chủ (sai phạm theo sổ kiểm toán).
 *
 * Chạy:  java -cp out MoPhong <host> <cổng> [--n 200] [--k 200] [--lan 5] [--che-do khoa] [--dat-lai]
 *                                [--so-sanh] [--tre-ms 2]
 */
public class MoPhong {
    // ===================================================================== NGHIỆP VỤ (MR6.7 – đặt xe)
    static final int SO_LAN_MAC_DINH = 1;                          // mỗi người bấm "Đặt xe" một lần
    static final int CONG_MAC_DINH = 43710;
    static Map<String, Object> thamSoDatLai() { return Json.obj(); }   // giữ số xe hiện tại của máy chủ

    /** Số xe còn trống mà một kết nối biết – đọc từ cap_nhat (có thể đã cũ khi tới máy chủ). */
    static final class NguCanh {
        volatile long soXe, trong;

        NguCanh(Map<String, Object> trangThai) {
            soXe = Chung.so(trangThai.get("so_xe"));
            trong = Chung.so(trangThai.get("trong"));
        }

        void capNhat(Map<String, Object> tin) {
            soXe = Chung.so(tin.get("so_xe"));
            trong = Chung.so(tin.get("trong"));
        }

        void nhanKq(Map<String, Object> tin) { if ("het_cho".equals(tin.get("ma_loi"))) trong = 0; }
    }

    /** Bấm "Đặt xe" từ một ngã tư ngẫu nhiên (máy chủ chọn xe trống gần nhất). Bị từ chối vì xung đột (lac_quan hết
     *  lượt thử) thì bấm lại, tối đa `lan` lần; "hết chỗ" hay đặt được thì thôi. */
    static final class NguoiAo {
        final String ten;
        final NguCanh nc;
        int con;

        NguoiAo(String ten, NguCanh nc, int lan) { this.ten = ten; this.nc = nc; this.con = lan; }

        private Map<String, Object> dat() {
            if (con <= 0) return null;
            con--;
            java.util.concurrent.ThreadLocalRandom r = ThreadLocalRandom.current();
            int i = r.nextInt(MayChu.NGA_TU.size()), j = r.nextInt(MayChu.NGA_TU.size() - 1);
            if (j >= i) j++;                                       // hai ngã tư khác nhau (như random.sample)
            return Json.obj("loai", "dat_xe", "nguoi", ten, "don", MayChu.dsDiem(MayChu.NGA_TU.get(i)),
                    "den", MayChu.dsDiem(MayChu.NGA_TU.get(j)));
        }

        Map<String, Object> yeuCauDau() { return dat(); }

        Map<String, Object> sauKq(Map<String, Object> tin) {
            if (Chung.dung(tin.get("ok")) || "het_cho".equals(tin.get("ma_loi")) || "da_dat".equals(tin.get("ma_loi"))) return null;
            return dat();
        }
    }

    // ===================================================================== KHUNG (giống nhau ở các bài)
    static final class KetQuaKetNoi {
        final List<Double> rtt = new ArrayList<>();
        long ok, tuChoi;
        final Map<String, Long> lyDo = new LinkedHashMap<>();
        volatile String loi;
        volatile long tXong;
    }

    /** Luồng của một kết nối: chào -> chờ rào -> gửi lô yêu cầu đầu -> đọc, gửi tiếp tới khi xong. */
    static void chayKetNoi(String host, int cong, int i, List<String> dsTen, int lan, CyclicBarrier rao, KetQuaKetNoi kq) {
        KetNoiKhach kn = new KetNoiKhach();
        try {
            kn.ketNoi(host, cong, "mp" + (i + 1), "nguoi_dung", (long) dsTen.size(), 5.0);
            NguCanh nc = new NguCanh(kn.trangThai);
            kn.khiCapNhat = nc::capNhat;
            PhienKetNoi ph = new PhienKetNoi(kn, nc, kq, dsTen.size());
            List<NguoiAo> dsNguoi = new ArrayList<>();
            for (String ten : dsTen) dsNguoi.add(new NguoiAo(ten, nc, lan));
            rao.await();                                             // RÀO XUẤT PHÁT: mọi kết nối sẵn sàng mới bắn
            List<KetNoiKhach.Yc> lo = new ArrayList<>();
            for (NguoiAo na : dsNguoi) {
                KetNoiKhach.Yc y = ph.gui(na, na.yeuCauDau());
                if (y != null) lo.add(y);
            }
            if (!lo.isEmpty()) kn.guiLo(lo);
            kn.vongDoc(() -> ph.con == 0);
            if (ph.con > 0) kq.loi = "máy chủ đóng kết nối giữa chừng";
        } catch (Exception e) {                                      // lỗi trước rào: phá rào để luồng chính biết
            kq.loi = e.getMessage() != null ? e.getMessage() : e.getClass().getSimpleName();
            rao.reset();
        } finally {
            kq.tXong = System.nanoTime();
            kn.dong();
        }
    }

    /** Việc của một kết nối sau rào: gửi yêu cầu kế tiếp của từng người khi trả lời của người đó tới. */
    static final class PhienKetNoi {
        final KetNoiKhach kn;
        final NguCanh nc;
        final KetQuaKetNoi kq;
        int con;                                                     // số người chưa xong

        PhienKetNoi(KetNoiKhach kn, NguCanh nc, KetQuaKetNoi kq, int con) {
            this.kn = kn; this.nc = nc; this.kq = kq; this.con = con;
        }

        /** yc null -> người đó xong; không thì một Yc (loại, trường, hàm nhận kết quả). */
        KetNoiKhach.Yc gui(NguoiAo na, Map<String, Object> yc) {
            if (yc == null) {
                con--;
                return null;
            }
            String loai = (String) yc.remove("loai");
            return new KetNoiKhach.Yc(loai, yc, (tin, rtt) -> nhan(na, tin, rtt));
        }

        /** Chạy trên chính luồng kết nối (trong vongDoc). */
        void nhan(NguoiAo na, Map<String, Object> tin, double rtt) {
            kq.rtt.add(rtt);
            if (Chung.dung(tin.get("ok"))) kq.ok++;
            else {
                kq.tuChoi++;
                Object m = tin.get("ma_loi");
                kq.lyDo.merge(m == null ? "khac" : m.toString(), 1L, Long::sum);
            }
            nc.nhanKq(tin);
            KetNoiKhach.Yc tiep = gui(na, na.sauKq(tin));
            if (tiep != null) {
                try { kn.guiYc(tiep.loai(), tiep.khiKq(), tiep.truong()); } catch (IOException e) { throw new RuntimeException(e); }
            }
        }
    }

    /** Một lượt mô phỏng; trả về một hàng của bảng so sánh (Map) hoặc null nếu lỗi. */
    static Map<String, Object> chayLuot(String host, int cong, int n, int k, int lan, Consumer<String> bao, String cheDo,
                                        boolean datLai, Double treMs) throws IOException {
        k = Math.max(1, Math.min(k, n));
        QuanTri qt = new QuanTri(host, cong, "mo_phong");
        try {
            if (treMs != null) qt.tre(treMs);
            if (datLai) qt.datLai(thamSoDatLai());                  // đặt lại trước (máy chủ in tổng kết lượt trước)
            if (cheDo != null) qt.cheDo(cheDo);
            cheDo = String.valueOf(qt.thongKe().get("che_do"));
            bao.accept("=== " + cheDo + ": " + n + " người × " + lan + " lần, " + k + " kết nối ===");
            List<List<String>> nhom = new ArrayList<>();
            for (int i = 0; i < k; i++) nhom.add(new ArrayList<>());
            for (int i = 0; i < n; i++) nhom.get(i % k).add("u" + (i + 1));
            CyclicBarrier rao = new CyclicBarrier(k + 1);
            List<KetQuaKetNoi> ketQua = new ArrayList<>();
            List<Thread> luong = new ArrayList<>();
            for (int i = 0; i < k; i++) {
                KetQuaKetNoi r = new KetQuaKetNoi();
                ketQua.add(r);
                int ii = i;
                luong.add(Thread.ofPlatform().daemon().name("mp" + (i + 1))
                        .start(() -> chayKetNoi(host, cong, ii, nhom.get(ii), lan, rao, r)));
            }
            try {
                rao.await(60, TimeUnit.SECONDS);
            } catch (BrokenBarrierException | TimeoutException | InterruptedException e) {
                String loi = "hết thời gian chờ";
                for (KetQuaKetNoi r : ketQua) if (r.loi != null) { loi = r.loi; break; }
                bao.accept("Không mở được đủ " + k + " kết nối: " + loi);
                return null;
            }
            long t0 = System.nanoTime();
            bao.accept("Rào xuất phát: " + k + "/" + k + " kết nối sẵn sàng -> bắn");
            for (Thread t : luong) {
                try { t.join(); } catch (InterruptedException ignored) { }
            }
            long t1 = 0;
            List<String> loi = new ArrayList<>();
            List<Double> rtt = new ArrayList<>();
            long ok = 0, tuChoi = 0;
            Map<String, Long> lyDo = new LinkedHashMap<>();
            for (KetQuaKetNoi r : ketQua) {
                t1 = Math.max(t1, r.tXong);
                if (r.loi != null) loi.add(r.loi);
                rtt.addAll(r.rtt);
                ok += r.ok;
                tuChoi += r.tuChoi;
                r.lyDo.forEach((m, s) -> lyDo.merge(m, s, Long::sum));
            }
            if (!loi.isEmpty()) bao.accept(loi.size() + " kết nối lỗi: " + loi.get(0));
            Collections.sort(rtt);
            long tong = ok + tuChoi;
            double giay = Math.max((t1 - t0) / 1e9, 1e-6);
            long ycS = Math.round(tong / giay);
            Double p50 = Chung.phanVi(rtt, 50), p95 = Chung.phanVi(rtt, 95), p99 = Chung.phanVi(rtt, 99);
            bao.accept(tong + " yc xong trong " + Chung.f("%.2f", giay) + " s = " + ycS + " yc/s");
            bao.accept("Khứ hồi ms: p50 " + ms1(p50) + " p95 " + ms1(p95) + " p99 " + ms1(p99));
            bao.accept("Thành công " + ok + ", từ chối " + tuChoi);
            if (!lyDo.isEmpty()) {
                List<Map.Entry<String, Long>> ds = new ArrayList<>(lyDo.entrySet());
                ds.sort((a, b) -> Long.compare(b.getValue(), a.getValue()));      // sắp xếp ổn định: như most_common()
                List<String> p = new ArrayList<>();
                for (Map.Entry<String, Long> e : ds) p.add(e.getKey() + " " + e.getValue());
                bao.accept("  lý do: " + String.join(", ", p));
            }
            Map<String, Object> tk = qt.thongKe();
            bao.accept("Máy chủ: xung đột " + Chung.py(tk.get("xung_dot")) + ", thử lại " + Chung.py(tk.get("thu_lai"))
                    + ", hàng đợi max " + Chung.py(tk.get("hang_doi_max")));
            bao.accept("Máy chủ xử lý µs: p50 " + Chung.soUs(tk.get("p50_us")) + " p95 " + Chung.soUs(tk.get("p95_us"))
                    + " p99 " + Chung.soUs(tk.get("p99_us")));
            bao.accept(Chung.dongSaiPham(tk));
            List<Object> viDu = Chung.list(tk.get("vi_du_sai_pham"));
            for (int i = 0; i < Math.min(2, viDu.size()); i++) bao.accept("  ví dụ: " + viDu.get(i));
            bao.accept(String.valueOf(tk.get("tom_tat")));
            return Json.obj("che_do", cheDo, "thanh_cong", ok, "sai_pham", tk.get("sai_pham_tong"),
                    "xung_dot", tk.get("xung_dot"), "yc_s", ycS, "p99_ms", p99, "tu_choi", tuChoi,
                    "tom_tat", tk.get("tom_tat"));
        } finally {
            qt.dong();
        }
    }

    static String ms1(Double v) { return v == null ? "–" : Chung.f("%.1f", v); }

    /** Chạy lần lượt 4 chế độ (đặt lại trạng thái trước mỗi lần); trả về các hàng; trả lại chế độ cũ. */
    static List<Map<String, Object>> soSanh(String host, int cong, int n, int k, int lan, Consumer<String> bao, Double treMs)
            throws IOException {
        QuanTri qt = new QuanTri(host, cong, "so_sanh");
        try {
            String goc = String.valueOf(qt.thongKe().get("che_do"));
            List<Map<String, Object>> hang = new ArrayList<>();
            for (String cd : Chung.CHE_DO) {
                Map<String, Object> h = chayLuot(host, cong, n, k, lan, bao, cd, true, treMs);
                if (h == null) break;
                hang.add(h);
            }
            qt.cheDo(goc);
            return hang;
        } finally {
            qt.dong();
        }
    }

    public static void main(String[] args) throws IOException {
        List<String> vt = new ArrayList<>();
        int n = 200, lan = SO_LAN_MAC_DINH;
        Integer k = null;
        String cheDo = null;
        boolean datLai = false, ss = false;
        Double treMs = null;
        for (int i = 0; i < args.length; i++) {
            switch (args[i]) {
                case "--n" -> n = Integer.parseInt(args[++i]);
                case "--k" -> k = Integer.parseInt(args[++i]);
                case "--lan" -> lan = Integer.parseInt(args[++i]);
                case "--che-do" -> {
                    cheDo = args[++i];
                    if (!Chung.CHE_DO.contains(cheDo)) throw new IllegalArgumentException("--che-do: " + Chung.CHE_DO);
                }
                case "--dat-lai" -> datLai = true;
                case "--so-sanh" -> ss = true;
                case "--tre-ms" -> treMs = Double.parseDouble(args[++i]);
                default -> vt.add(args[i]);
            }
        }
        String host = vt.size() > 0 ? vt.get(0) : "127.0.0.1";
        int cong = vt.size() > 1 ? Integer.parseInt(vt.get(1)) : CONG_MAC_DINH;
        int kk = k != null && k > 0 ? k : Math.min(n, 200);
        Consumer<String> bao = text -> Chung.inDong("mo_phong", text);
        if (ss) {
            List<Map<String, Object>> hang = soSanh(host, cong, n, kk, lan, bao, treMs);
            bao.accept("Bảng so sánh (" + n + " người × " + lan + " lần, " + Math.min(kk, n) + " kết nối):");
            for (String d : Chung.bangSoSanh(hang)) Chung.inTho(d);
        } else {
            chayLuot(host, cong, n, kk, lan, bao, cheDo, datLai, treMs);
        }
        System.exit(0);
    }
}
