import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR5.6 – Bộ lập lịch mô phỏng: đơn luồng/đa luồng × đồng bộ/bất đồng bộ.
 *
 * Chuyển đúng từ bản Python (lap_lich.py) để in trùng từng ký tự. Thời gian rời rạc, 1 nhịp = 100 ms.
 * Mỗi tác vụ T1, T2, T3 có thời lượng D giây, gồm ba bước:
 *   CPU (D/4) → chờ I/O (D/2, ví dụ đọc mạng hay đĩa) → CPU (phần còn lại).
 * Tùy chọn:
 *   - phụ thuộc: T3 chỉ bắt đầu khi T1 và T2 đã xong (join / CompletableFuture.allOf);
 *   - chia: bước CPU cuối của T1 được chia đôi, hai nửa chạy cùng lúc trên hai luồng rồi hợp lại;
 *   - việc khẩn U: đến khi các luồng đang bận (mặc định lúc 0,3 s), cần 0,5 s CPU, ưu tiên cao;
 *   - số lõi CPU và lát thời gian (quantum) của bộ lập lịch hệ điều hành.
 *
 * Bốn chế độ:
 *   don_dong_bo      một luồng, lời gọi chặn: làm lần lượt theo thứ tự đến (T1, T2, T3 rồi U);
 *   da_dong_bo       mỗi tác vụ một luồng hệ điều hành: tranh lõi, lát thời gian, chen ngang (preemption),
 *                    T3 chờ T1, T2 bằng join, nửa sau phần CPU của T1 chạy trên luồng phụ T1b;
 *   don_bat_dong_bo  một luồng chạy vòng lặp sự kiện: bước CPU chạy tới khi xong (hợp tác), chờ I/O không
 *                    giữ luồng nên các I/O chồng lên nhau; hàng việc FIFO;
 *   da_bat_dong_bo   nhóm luồng thợ (số thợ = số lõi): mỗi bước CPU giao cho thợ rảnh đầu tiên, sau một lần
 *                    chờ I/O tác vụ có thể chạy tiếp trên MỘT LUỒNG KHÁC (như CompletableFuture trên ForkJoinPool).
 * Mỗi nhịp gồm bốn bước theo thứ tự cố định: (1) tác vụ đến, (2) các luồng/tác vụ tiến sang thao tác kế tiếp,
 * (3) lập lịch (cấp lõi, giao việc cho thợ), (4) chạy một nhịp và ghi lại. Kết quả tất định.
 *
 * Lớp này không tạo luồng thật nào: luồng, lõi CPU, vòng lặp sự kiện đều là dữ liệu mô phỏng.
 */
final class LapLich {
    private LapLich() { }

    static final Map<String, String> CHE_DO = new LinkedHashMap<>();
    static final Map<String, String> TEN_NGAN = new HashMap<>();
    static {
        CHE_DO.put("don_dong_bo", "Đơn luồng – đồng bộ (Single-threaded Sync)");
        CHE_DO.put("da_dong_bo", "Đa luồng – đồng bộ (Multi-threaded Sync)");
        CHE_DO.put("don_bat_dong_bo", "Đơn luồng – bất đồng bộ (Single-threaded Async)");
        CHE_DO.put("da_bat_dong_bo", "Đa luồng – bất đồng bộ (Multi-threaded Async)");
        TEN_NGAN.put("don_dong_bo", "đơn, đồng bộ");
        TEN_NGAN.put("da_dong_bo", "đa, đồng bộ");
        TEN_NGAN.put("don_bat_dong_bo", "đơn, bất đ.bộ");
        TEN_NGAN.put("da_bat_dong_bo", "đa, bất đ.bộ");
    }

    // trạng thái một ô (một làn, một nhịp)
    static final String CPU = "cpu", IO = "io", SAN_SANG = "san_sang", CHO = "cho", RANH = "ranh";
    static final int U_CPU = 5;                       // việc khẩn: 0,5 s CPU

    /** Số nhịp (100 ms) → chuỗi giây, một chữ số thập phân (chia nguyên làm tròn xuống như // của Python). */
    static String giay(int nhip) {
        return Math.floorDiv(nhip, 10) + "." + Math.floorMod(nhip, 10);
    }

    /** round() của Python: làm tròn về số chẵn gần nhất khi đúng nửa (banker's rounding) – Math.rint. */
    static int lamTron(double x) {
        return (int) Math.rint(x);
    }

    /** Căn trái/phải theo số ký tự (code point), như f"{s:<n}" / f"{s:>n}" của Python. */
    static String trai(String s, int n) {
        int k = s.codePointCount(0, s.length());
        return k >= n ? s : s + " ".repeat(n - k);
    }

    static String phai(String s, int n) {
        int k = s.codePointCount(0, s.length());
        return k >= n ? s : " ".repeat(n - k) + s;
    }

    static final class ThamSo {
        final int[] d;                                // nhịp
        final int soLoi, lat, tKhan;
        final boolean phuThuoc, chia, khan;

        ThamSo(double t1, double t2, double t3, int soLoi, double lat, boolean phuThuoc, boolean chia,
               boolean khan, double tKhan) {
            this.d = new int[] {lamTron(t1 * 10), lamTron(t2 * 10), lamTron(t3 * 10)};
            this.soLoi = Math.max(1, soLoi);
            this.lat = Math.max(1, lamTron(lat * 10));
            this.phuThuoc = phuThuoc;
            this.chia = chia;
            this.khan = khan;
            this.tKhan = lamTron(tKhan * 10);
        }

        ThamSo() { this(4.0, 3.0, 2.0, 2, 0.5, true, true, true, 0.3); }
    }

    /** Một bước của tác vụ: loại = cpu | io | chia (CPU chia đôi chạy song song), số nhịp. */
    record Buoc(String loai, int n) { }

    /** Một tác vụ. */
    static final class Viec {
        final String ten;
        final List<Buoc> buoc;
        final int uuTien, den;
        final List<String> phuThuoc;
        int batDau = -1, xong = -1;                   // nhịp đầu tiên dùng CPU, nhịp hoàn tất
        int k = 0;                                    // (bất đồng bộ) bước đang làm
        int conCho = 0;                               // (bất đồng bộ) số nửa của bước chia còn chưa xong
        boolean daChay = false;                       // (bất đồng bộ) đã bắt đầu

        Viec(String ten, List<Buoc> buoc, int uuTien, int den, List<String> phuThuoc) {
            this.ten = ten;
            this.buoc = buoc;
            this.uuTien = uuTien;
            this.den = den;
            this.phuThuoc = phuThuoc;
        }
    }

    static List<Viec> taoViec(ThamSo ts) {
        List<Viec> ds = new ArrayList<>();
        for (int i = 0; i < 3; i++) {
            int n = ts.d[i];
            int c1 = Math.floorDiv(n, 4), io = Math.floorDiv(n, 2);
            int c2 = n - c1 - io;
            List<Buoc> buoc = new ArrayList<>();
            for (Buoc b : List.of(new Buoc("cpu", c1), new Buoc("io", io),
                    new Buoc(ts.chia && i == 0 && c2 >= 2 ? "chia" : "cpu", c2))) {
                if (b.n() > 0) buoc.add(b);
            }
            List<String> dep = ts.phuThuoc && i == 2 ? List.of("T1", "T2") : List.of();
            ds.add(new Viec("T" + (i + 1), buoc, 0, 0, dep));
        }
        if (ts.khan) ds.add(new Viec("U", List.of(new Buoc("cpu", U_CPU)), 1, ts.tKhan, List.of()));
        return ds;
    }

    /** Một ô của biểu đồ: trạng thái và nhãn (tên tác vụ). Record nên so sánh bằng nội dung. */
    record O(String tt, String nhan) { }

    static final O O_RANH = new O(RANH, "");

    /** Một làn (một luồng) của biểu đồ. */
    record Lan(String ten, List<O> o) { }

    /** Một dòng nhật ký. */
    record Ghi(int t, String text) { }

    /** Kết quả một lần chạy. */
    static final class KetQua {
        final String cheDo;
        final ThamSo ts;
        List<Lan> lan = new ArrayList<>();
        final List<Integer> io = new ArrayList<>();   // số I/O bất đồng bộ đang chờ mỗi nhịp
        final List<Ghi> nhatKy = new ArrayList<>();
        List<Viec> viec = new ArrayList<>();
        int chenNgang = 0, hetLat = 0, tong = 0;

        KetQua(String cheDo, ThamSo ts) {
            this.cheDo = cheDo;
            this.ts = ts;
        }

        void log(int t, String text) { nhatKy.add(new Ghi(t, text)); }
    }

    // ==================================================================== đồng bộ (luồng hệ điều hành)
    /** Một thao tác trong chương trình của luồng. */
    static final class Op {
        final String loai;            // cpu | io | cho_viec | tao_phu | cho_phu | ghi | xong | het
        int n;                        // số nhịp còn lại (cpu, io, tao_phu)
        List<String> deps;            // cho_viec
        Luong phu;                    // cho_phu
        String text;                  // ghi
        final String ten;
        final Viec v;

        Op(String loai, int n, String ten, Viec v) {
            this.loai = loai;
            this.n = n;
            this.ten = ten;
            this.v = v;
        }
    }

    static final class Luong {
        final String ten;
        final int uuTien;
        List<Op> ct;                  // chương trình: các thao tác chưa làm
        Op op = null;                 // thao tác đang làm
        boolean ketThuc = false;
        String trangThai = RANH;
        int latDaChay = 0;
        final List<O> o = new ArrayList<>();

        Luong(String ten, int uuTien, List<Op> ct, int t) {
            this.ten = ten;
            this.uuTien = uuTien;
            this.ct = ct;
            for (int i = 0; i < t; i++) o.add(O_RANH);
        }
    }

    /** Một chỗ trong hàng sẵn sàng: luồng và số thứ tự vào hàng. */
    record SanSang(Luong l, int stt) { }

    static final class DongBo {
        final ThamSo ts;
        final boolean nhieu;
        final KetQua kq;
        final List<Viec> viec;
        final List<Luong> luong = new ArrayList<>();
        final List<SanSang> sanSang = new ArrayList<>();
        final List<Luong> dangChay = new ArrayList<>();
        int stt = 0;
        final List<Viec> hang = new ArrayList<>();    // đơn luồng: tác vụ đã đến, chờ luồng chính (FIFO)
        Luong chinh = null;
        int t = 0;

        DongBo(ThamSo ts, boolean nhieuLuong) {
            this.ts = ts;
            this.nhieu = nhieuLuong;
            this.kq = new KetQua(nhieuLuong ? "da_dong_bo" : "don_dong_bo", ts);
            this.viec = taoViec(ts);
            kq.viec = viec;
        }

        void log(String text) { kq.log(t, text); }

        boolean daXong(List<String> deps) {
            for (String d : deps) {
                boolean co = false;
                for (Viec v : viec) if (v.ten.equals(d) && v.xong >= 0) { co = true; break; }
                if (!co) return false;
            }
            return true;
        }

        /** Chương trình của tác vụ v: [chờ phụ thuộc], các bước, xong. */
        List<Op> ctViec(Viec v, boolean motLuong) {
            List<Op> ct = new ArrayList<>();
            if (!v.phuThuoc.isEmpty() && !motLuong) {
                Op op = new Op("cho_viec", 0, v.ten, v);
                op.deps = new ArrayList<>(v.phuThuoc);
                ct.add(op);
            }
            for (Buoc b : v.buoc) {
                if (b.loai().equals("chia")) {
                    int a = Math.floorDiv(b.n() + 1, 2), c = Math.floorDiv(b.n(), 2);
                    if (motLuong) {
                        Op g = new Op("ghi", 0, v.ten, v);
                        g.text = v.ten + ": chia CPU cuối 2 nửa, một luồng nên làm lần lượt";
                        ct.add(g);
                        ct.add(new Op("cpu", a, v.ten, v));
                        ct.add(new Op("cpu", c, v.ten, v));
                    } else {
                        ct.add(new Op("tao_phu", c, v.ten, v));
                        ct.add(new Op("cpu", a, v.ten, v));
                        ct.add(new Op("cho_phu", 0, v.ten, v));
                    }
                } else {
                    ct.add(new Op(b.loai(), b.n(), v.ten, v));
                }
            }
            ct.add(new Op("xong", 0, v.ten, v));
            return ct;
        }

        Luong themLuong(String ten, int uuTien, List<Op> ct) {
            Luong l = new Luong(ten, uuTien, ct, t);
            luong.add(l);
            return l;
        }

        void vaoSanSang(Luong l) {
            stt += 1;
            sanSang.add(new SanSang(l, stt));
        }

        boolean dangChayCo(Luong l) {
            for (Luong r : dangChay) if (r == l) return true;
            return false;
        }

        boolean sanSangCo(Luong l) {
            for (SanSang s : sanSang) if (s.l() == l) return true;
            return false;
        }

        void roiLoi(Luong l) { dangChay.remove(l); }

        /** Bước 2: luồng l làm các thao tác tức thời và sang thao tác kế tiếp nếu thao tác hiện tại đã xong. */
        void tien(Luong l) {
            while (true) {
                Op op = l.op;
                if (op != null) {
                    String loai = op.loai;
                    if ((loai.equals("cpu") || loai.equals("io")) && op.n > 0) return;
                    if (loai.equals("cho_viec")) {
                        if (!daXong(op.deps)) return;
                        log(l.ten + ": " + String.join(" và ", op.deps) + " đã xong (join), " + op.ten + " chạy tiếp");
                    }
                    if (loai.equals("cho_phu")) {
                        if (!op.phu.ketThuc) return;
                        log(l.ten + ": hai nửa của " + op.ten + " đã xong (join " + op.phu.ten + ")");
                    }
                }
                if (l.ct.isEmpty()) {
                    if (!nhieu && l == chinh && !hang.isEmpty()) {
                        Viec v = hang.remove(0);
                        log("Luồng chính bắt đầu " + v.ten);
                        l.ct = ctViec(v, true);
                    } else {
                        l.op = null;
                        roiLoi(l);
                        boolean daDen = true;
                        for (Viec v : viec) if (v.den > t) daDen = false;
                        if (nhieu || daDen) l.ketThuc = true;
                        l.trangThai = RANH;
                        return;
                    }
                }
                l.op = l.ct.remove(0);
                String loai = l.op.loai;
                if (loai.equals("cpu")) {
                    if (!dangChayCo(l) && !sanSangCo(l)) vaoSanSang(l);
                    l.trangThai = dangChayCo(l) ? CPU : SAN_SANG;
                    return;
                }
                roiLoi(l);
                switch (loai) {
                    case "io" -> {
                        l.trangThai = IO;
                        return;
                    }
                    case "cho_viec" -> {
                        l.trangThai = CHO;
                        if (!daXong(l.op.deps))
                            log(l.ten + ": " + l.op.ten + " chờ " + String.join(" và ", l.op.deps) + " xong (join)");
                    }
                    case "tao_phu" -> {
                        List<Op> ctPhu = new ArrayList<>();
                        ctPhu.add(new Op("cpu", l.op.n, l.op.ten, l.op.v));
                        ctPhu.add(new Op("het", 0, l.op.ten, l.op.v));
                        Luong phu = themLuong("Luồng " + l.op.ten + "b", l.uuTien, ctPhu);
                        log(l.op.ten + ": chia CPU cuối 2 nửa, " + phu.ten + " chạy nửa sau");
                        l.ct.get(1).phu = phu;            // thao tác cho_phu chờ đúng luồng phụ này
                    }
                    case "cho_phu" -> l.trangThai = CHO;
                    case "ghi" -> log(l.op.text);
                    case "xong" -> {
                        Viec v = l.op.v;
                        v.xong = t;
                        log(v.ten + " xong lúc " + giay(t) + " s");
                    }
                    default -> { }                        // het
                }
            }
        }

        SanSang totNhat() {                               // ưu tiên cao, rồi đến trước
            SanSang best = null;
            for (SanSang s : sanSang) {
                if (best == null || s.l().uuTien > best.l().uuTien
                        || (s.l().uuTien == best.l().uuTien && s.stt() < best.stt())) best = s;
            }
            return best;
        }

        /** Bước 3: bộ lập lịch hệ điều hành cấp lõi CPU cho các luồng sẵn sàng. */
        void lapLich() {
            // a. hết lát thời gian: nhường lõi nếu có luồng ưu tiên bằng hoặc cao hơn đang chờ
            for (Luong r : new ArrayList<>(dangChay)) {
                boolean coCho = false;
                for (SanSang s : sanSang) if (s.l().uuTien >= r.uuTien) { coCho = true; break; }
                if (r.latDaChay >= ts.lat && coCho) {
                    dangChay.remove(r);
                    r.latDaChay = 0;
                    vaoSanSang(r);
                    kq.hetLat += 1;
                    log(r.ten + " hết lát thời gian " + giay(ts.lat) + " s, nhường lõi");
                }
            }
            // b. lõi còn trống: cấp cho luồng sẵn sàng tốt nhất
            while (dangChay.size() < ts.soLoi && !sanSang.isEmpty()) {
                SanSang s = totNhat();
                sanSang.remove(s);
                s.l().latDaChay = 0;
                dangChay.add(s.l());
            }
            // c. chen ngang: luồng sẵn sàng ưu tiên cao hơn luồng đang chạy thì lấy lõi ngay
            while (!sanSang.isEmpty()) {
                SanSang s = totNhat();
                // nạn nhân: ưu tiên thấp nhất, rồi đã chạy nhiều nhất trong lát, rồi vị trí đầu
                Luong nan = null;
                int iNan = -1;
                for (int i = 0; i < dangChay.size(); i++) {
                    Luong r = dangChay.get(i);
                    if (r.uuTien >= s.l().uuTien) continue;
                    if (nan == null || r.uuTien < nan.uuTien
                            || (r.uuTien == nan.uuTien && r.latDaChay > nan.latDaChay)) {
                        nan = r;
                        iNan = i;
                    }
                }
                if (nan == null) break;
                sanSang.remove(s);
                s.l().latDaChay = 0;
                dangChay.set(iNan, s.l());
                nan.latDaChay = 0;
                vaoSanSang(nan);
                kq.chenNgang += 1;
                log(s.l().ten + " CHEN NGANG: " + nan.ten + " tạm dừng, nhường lõi");
            }
            for (Luong l : luong) {
                if (l.op != null && l.op.loai.equals("cpu") && l.op.n > 0)
                    l.trangThai = dangChayCo(l) ? CPU : SAN_SANG;
            }
        }

        KetQua chay() {
            while (true) {
                // 1. tác vụ đến
                for (Viec v : viec) {
                    if (v.den == t) {
                        if (v.uuTien != 0) log(v.ten + " (việc khẩn, ưu tiên cao) đến");
                        if (nhieu) {
                            Luong l = themLuong("Luồng " + v.ten, v.uuTien, ctViec(v, false));
                            if (v.uuTien != 0) log("tạo " + l.ten);
                        } else {
                            hang.add(v);
                            if (chinh == null) chinh = themLuong("Luồng chính", 0, new ArrayList<>());
                            else if (v.uuTien != 0 && (chinh.op != null || !chinh.ct.isEmpty()))
                                log(v.ten + " xếp hàng: luồng chính bận, không chen ngang được");
                        }
                    }
                }
                // 2. tiến (theo thứ tự tạo luồng; luồng mới tạo trong bước này cũng được tiến). Hai lượt để
                //    luồng đang join thấy ngay luồng có số thứ tự lớn hơn vừa kết thúc trong cùng nhịp.
                for (int lan = 0; lan < 2; lan++) {
                    for (int i = 0; i < luong.size(); i++) {
                        if (!luong.get(i).ketThuc) tien(luong.get(i));
                    }
                }
                boolean het = true;
                for (Luong l : luong) if (!l.ketThuc) het = false;
                for (Viec v : viec) if (v.den > t) het = false;
                if (het) break;
                // 3. lập lịch
                lapLich();
                // 4. chạy một nhịp
                for (Luong l : luong) {
                    Op op = l.op;
                    if (l.ketThuc || op == null) {
                        l.o.add(O_RANH);
                    } else if (l.trangThai.equals(CPU)) {
                        op.n -= 1;
                        l.latDaChay += 1;
                        if (op.v.batDau < 0) op.v.batDau = t;
                        l.o.add(new O(CPU, op.ten));
                    } else if (l.trangThai.equals(IO)) {
                        op.n -= 1;
                        l.o.add(new O(IO, op.ten));
                    } else {
                        l.o.add(new O(l.trangThai, op.ten));
                    }
                }
                t += 1;
            }
            kq.tong = t;
            kq.lan = new ArrayList<>();
            for (Luong l : luong) kq.lan.add(new Lan(l.ten, l.o));
            return kq;
        }
    }

    // ==================================================================== bất đồng bộ (vòng lặp / nhóm thợ)
    /** Một bước CPU trong hàng hoặc đang chạy trên thợ. */
    static final class ViecCpu {
        final Viec v;
        int n;
        final boolean nua;

        ViecCpu(Viec v, int n, boolean nua) {
            this.v = v;
            this.n = n;
            this.nua = nua;
        }
    }

    /** Một lần chờ I/O bất đồng bộ (không giữ luồng nào). */
    static final class ChoIo {
        final Viec v;
        int n;

        ChoIo(Viec v, int n) {
            this.v = v;
            this.n = n;
        }
    }

    static final class BatDongBo {
        final ThamSo ts;
        final boolean nhieu;
        final KetQua kq;
        final List<Viec> viec;
        final List<String> tenTho = new ArrayList<>();
        final ViecCpu[] tho;                              // việc đang chạy trên mỗi thợ
        final List<List<O>> o = new ArrayList<>();
        final List<ViecCpu> hang = new ArrayList<>();     // các bước CPU sẵn sàng, FIFO
        final List<ChoIo> choIo = new ArrayList<>();
        final Map<String, Integer> cuoiTho = new HashMap<>();   // tác vụ -> thợ chạy bước CPU gần nhất
        int t = 0;

        BatDongBo(ThamSo ts, boolean nhieuLuong) {
            this.ts = ts;
            this.nhieu = nhieuLuong;
            this.kq = new KetQua(nhieuLuong ? "da_bat_dong_bo" : "don_bat_dong_bo", ts);
            this.viec = taoViec(ts);
            kq.viec = viec;
            int soTho = nhieuLuong ? ts.soLoi : 1;
            if (nhieuLuong) for (int i = 0; i < soTho; i++) tenTho.add("Thợ " + (i + 1));
            else tenTho.add("Vòng lặp");
            tho = new ViecCpu[soTho];
            for (int i = 0; i < soTho; i++) o.add(new ArrayList<>());
        }

        void log(String text) { kq.log(t, text); }

        /** Tác vụ v bắt đầu bước v.k (hoặc hoàn tất nếu hết bước). */
        void batDauBuoc(Viec v) {
            if (v.k >= v.buoc.size()) {
                v.xong = t;
                log(v.ten + " xong lúc " + giay(t) + " s");
                for (Viec w : viec) {            // tác vụ đang chờ (await) các tác vụ vừa xong
                    if (!w.daChay && w.den <= t && !w.phuThuoc.isEmpty() && duPhuThuoc(w)) {
                        log(String.join(" và ", w.phuThuoc) + " đã xong → " + w.ten + " bắt đầu (await)");
                        khoiDong(w);
                    }
                }
                return;
            }
            Buoc b = v.buoc.get(v.k);
            switch (b.loai()) {
                case "cpu" -> hang.add(new ViecCpu(v, b.n(), false));
                case "io" -> choIo.add(new ChoIo(v, b.n()));
                default -> {                     // chia: hai nửa vào hàng cùng lúc
                    v.conCho = 2;
                    hang.add(new ViecCpu(v, Math.floorDiv(b.n() + 1, 2), true));
                    hang.add(new ViecCpu(v, Math.floorDiv(b.n(), 2), true));
                    if (tho.length > 1) log(v.ten + ": chia CPU cuối 2 nửa, chạy song song trên 2 thợ");
                    else log(v.ten + ": chia CPU cuối 2 nửa, một luồng nên làm lần lượt");
                }
            }
        }

        boolean duPhuThuoc(Viec v) {
            for (String d : v.phuThuoc) {
                boolean co = false;
                for (Viec w : viec) if (w.ten.equals(d) && w.xong >= 0) { co = true; break; }
                if (!co) return false;
            }
            return true;
        }

        void khoiDong(Viec v) {
            v.daChay = true;
            v.k = 0;
            batDauBuoc(v);
        }

        KetQua chay() {
            int soTho = tho.length;
            while (true) {
                // 1. tác vụ đến
                for (Viec v : viec) {
                    if (v.den == t) {
                        if (v.uuTien != 0) log(v.ten + " (việc khẩn) đến – xếp hàng, không chen ngang được");
                        if (!v.phuThuoc.isEmpty() && !duPhuThuoc(v))
                            log(v.ten + " chờ " + String.join(" và ", v.phuThuoc) + " xong (await), không giữ luồng nào");
                        else khoiDong(v);
                    }
                }
                // 2. tiến: bước CPU xong (theo thứ tự thợ), rồi I/O xong (theo thứ tự bắt đầu chờ)
                for (int i = 0; i < soTho; i++) {
                    ViecCpu x = tho[i];
                    if (x != null && x.n == 0) {
                        tho[i] = null;
                        Viec v = x.v;
                        if (x.nua) {
                            v.conCho -= 1;
                            if (v.conCho == 0) {
                                log(v.ten + ": hai nửa đã xong, hợp kết quả");
                                v.k += 1;
                                batDauBuoc(v);
                            }
                        } else {
                            v.k += 1;
                            batDauBuoc(v);
                        }
                    }
                }
                for (ChoIo x : new ArrayList<>(choIo)) {
                    if (x.n == 0) {
                        choIo.remove(x);
                        x.v.k += 1;
                        batDauBuoc(x.v);
                    }
                }
                boolean het = true;
                for (Viec v : viec) if (v.xong < 0) het = false;
                if (het) break;
                // 3. giao việc: thợ rảnh (theo thứ tự) lấy bước CPU đứng đầu hàng
                for (int i = 0; i < soTho; i++) {
                    if (tho[i] == null && !hang.isEmpty()) {
                        ViecCpu x = hang.remove(0);
                        tho[i] = x;
                        Integer truoc = cuoiTho.get(x.v.ten);
                        if (nhieu && truoc != null && truoc != i && !x.nua)
                            log(x.v.ten + " chạy tiếp trên " + tenTho.get(i) + " (trước đó ở " + tenTho.get(truoc) + ")");
                        cuoiTho.put(x.v.ten, i);
                    }
                }
                // 4. chạy một nhịp
                for (int i = 0; i < soTho; i++) {
                    ViecCpu x = tho[i];
                    if (x == null) {
                        o.get(i).add(O_RANH);
                    } else {
                        x.n -= 1;
                        if (x.v.batDau < 0) x.v.batDau = t;
                        o.get(i).add(new O(CPU, x.v.ten));
                    }
                }
                kq.io.add(choIo.size());
                for (ChoIo x : choIo) x.n -= 1;
                t += 1;
            }
            kq.tong = t;
            kq.lan = new ArrayList<>();
            for (int i = 0; i < soTho; i++) kq.lan.add(new Lan(tenTho.get(i), o.get(i)));
            return kq;
        }
    }

    static KetQua chay(String cheDo, ThamSo ts) {
        return switch (cheDo) {
            case "don_dong_bo" -> new DongBo(ts, false).chay();
            case "da_dong_bo" -> new DongBo(ts, true).chay();
            case "don_bat_dong_bo" -> new BatDongBo(ts, false).chay();
            default -> new BatDongBo(ts, true).chay();
        };
    }

    // ==================================================================== chỉ số và biểu đồ văn bản
    /** Chỉ số: hoàn tất, độ trễ phản hồi của U (-1 nếu không có U), số luồng, % lõi bận, chen ngang, hết lát. */
    record ChiSo(int tong, int uCho, int luong, int ban, int chen, int lat) { }

    static ChiSo chiSo(KetQua kq) {
        int ban = 0;
        for (Lan l : kq.lan) for (O c : l.o()) if (c.tt().equals(CPU)) ban++;
        Viec u = null;
        for (Viec v : kq.viec) if (v.ten.equals("U")) { u = v; break; }
        return new ChiSo(kq.tong, u != null ? u.batDau - u.den : -1, kq.lan.size(),
                Math.floorDiv(ban * 100, Math.max(1, kq.ts.soLoi * kq.tong)), kq.chenNgang, kq.hetLat);
    }

    static String kyTu(O c) {
        if (c.tt().equals(CPU)) return c.nhan().equals("U") ? "U" : c.nhan().substring(1, 2);   // T1 → "1"
        return switch (c.tt()) {
            case IO -> "~";
            case SAN_SANG -> "-";
            case CHO -> ".";
            default -> " ";
        };
    }

    /** Biểu đồ Gantt văn bản, 1 ô = 1 nhịp (0,1 s); dài hơn rong nhịp thì xuống dòng thành nhiều khối. */
    static List<String> gantt(KetQua kq, int rong) {
        List<String> dong = new ArrayList<>();
        for (int dau = 0; dau < kq.tong; dau += rong) {
            int cuoi = Math.min(kq.tong, dau + rong);
            for (Lan l : kq.lan) {
                StringBuilder sb = new StringBuilder();
                for (int j = dau; j < Math.min(cuoi, l.o().size()); j++) sb.append(kyTu(l.o().get(j)));
                dong.add("  " + trai(l.ten(), 11) + "|" + sb + "|");
            }
            if (!kq.io.isEmpty()) {
                StringBuilder sb = new StringBuilder();
                for (int j = dau; j < Math.min(cuoi, kq.io.size()); j++) {
                    int n = kq.io.get(j);
                    sb.append(n != 0 ? String.valueOf(Math.min(n, 9)) : " ");
                }
                dong.add("  " + trai("I/O chờ", 11) + "|" + sb + "|");
            }
            String nhan = giay(cuoi) + " s";
            dong.add("  " + trai("", 11) + " " + giay(dau) + phai(nhan, cuoi - dau - giay(dau).length() + 2));
        }
        return dong;
    }

    static List<String> gantt(KetQua kq) { return gantt(kq, 48); }
}
