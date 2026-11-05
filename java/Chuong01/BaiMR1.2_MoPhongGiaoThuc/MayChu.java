import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR1.2 – Bên trong máy chủ: đồng bộ, đa luồng, nhóm luồng, bất đồng bộ và phối hợp.
 *
 * Sáu yêu cầu từ các client (H1, H2, H3 ở LAN A; H4, H6 ở LAN B) đi qua mạng mô phỏng tới máy chủ H5.
 * Mỗi yêu cầu cần đọc CSDL 30 ms (chờ I/O: luồng chờ nhưng không tốn CPU) rồi xử lý 10 ms CPU;
 * máy chủ có 2 lõi CPU. Cùng khối việc đó, máy chủ được tổ chức theo năm mô hình:
 *
 *   dong_bo      một luồng, xử lý lần lượt: yêu cầu sau phải chờ yêu cầu trước xong hẳn;
 *   da_luong     mỗi yêu cầu một luồng (tạo luồng 1 ms); luồng nhiều hơn lõi thì phải chờ lõi;
 *                cập nhật bộ đếm chung trong đoạn găng có khóa (3 ms) – luồng khác phải chờ khóa;
 *   nhom_luong   nhóm 2 luồng thợ + hàng đợi tối đa 2 yêu cầu; hàng đợi đầy thì từ chối ngay (503);
 *   bat_dong_bo  một luồng chạy vòng lặp sự kiện: I/O không chặn (nhiều I/O chờ cùng lúc), nhưng phần
 *                CPU chạy ngay trên vòng lặp nên vòng lặp bị chặn – nhịp tim 10 ms chạy muộn;
 *   phoi_hop     vòng lặp sự kiện lo I/O, đẩy phần CPU sang nhóm 2 luồng (trong Java: ExecutorService
 *                hoặc CompletableFuture.supplyAsync); luồng thợ trả kết quả về vòng lặp qua một hàng
 *                đợi an toàn luồng – vòng lặp luôn rảnh, nhịp tim đúng giờ.
 *
 * Mô hình giảng dạy đã đơn giản hóa: lõi CPU cấp theo thứ tự đến, không chia lát thời gian (không
 * preemption); đoạn găng chỉ tính thời gian giữ khóa; thời gian chuyển ngữ cảnh bằng 0.
 * Không tạo luồng thật: luồng, lõi CPU, khóa, vòng lặp sự kiện đều là sự kiện trong Mang.java.
 */
final class MayChu {
    static final long MS = 1000;
    static final long IO_US = 30 * MS;          // đọc CSDL: chờ I/O
    static final long CPU_US = 10 * MS;         // xử lý: dùng một lõi CPU
    static final long KHOA_US = 3 * MS;         // đoạn găng (mô hình đa luồng)
    static final long TAO_US = 1 * MS;          // tạo một luồng mới (mô hình đa luồng)
    static final long NHIP_US = 10 * MS;        // chu kỳ nhịp tim của vòng lặp sự kiện
    static final int SO_LOI = 2;                // số lõi CPU của máy chủ
    static final int SO_THO = 2;                // số luồng của nhóm luồng
    static final int HANG_TOI_DA = 2;           // sức chứa hàng đợi của nhóm luồng
    static final String SERVER = "H5";

    /** Một yêu cầu: mã, client, thời điểm gửi (µs). */
    record YeuCau(String label, String client, long t) {}

    static final List<YeuCau> YEU_CAU = List.of(
            new YeuCau("A", "H4", 0), new YeuCau("B", "H6", 0), new YeuCau("C", "H4", 2 * MS),
            new YeuCau("D", "H1", 0), new YeuCau("E", "H2", 0), new YeuCau("F", "H3", 0));

    static final Map<String, String> TITLES = new LinkedHashMap<>();
    static final Map<String, String> TEN_NGAN = new HashMap<>();
    static final Map<String, List<String>> NHAN_XET = new HashMap<>();
    static final Map<String, String> KY_HIEU = Map.of("khoa", "#", "cho_khoa", "~", "cho_cpu", "-", "tao", "+");

    static {
        TITLES.put("dong_bo", "Đồng bộ, một luồng – xử lý lần lượt, yêu cầu sau phải chờ");
        TITLES.put("da_luong", "Đa luồng – mỗi yêu cầu một luồng, chờ lõi CPU, khóa");
        TITLES.put("nhom_luong", "Nhóm luồng – 2 luồng thợ + hàng đợi 2, đầy thì từ chối (503)");
        TITLES.put("bat_dong_bo", "Bất đồng bộ – một vòng lặp sự kiện, việc CPU chặn vòng lặp");
        TITLES.put("phoi_hop", "Phối hợp – vòng lặp sự kiện lo I/O, nhóm luồng lo việc CPU");
        TEN_NGAN.put("dong_bo", "đồng bộ");
        TEN_NGAN.put("da_luong", "đa luồng");
        TEN_NGAN.put("nhom_luong", "nhóm luồng");
        TEN_NGAN.put("bat_dong_bo", "bất đồng bộ");
        TEN_NGAN.put("phoi_hop", "phối hợp");
        NHAN_XET.put("dong_bo", List.of(
                "Nhận xét: một luồng làm lần lượt; khi chờ CSDL luồng bị chặn,",
                "  CPU rảnh mà không phục vụ được ai; yêu cầu sau chờ cộng dồn."));
        NHAN_XET.put("da_luong", List.of(
                "Nhận xét: mỗi yêu cầu một luồng nên các lần chờ I/O chồng lên",
                "  nhau; nhưng 6 luồng chỉ có 2 lõi (-: chờ lõi), đoạn găng chỉ",
                "  cho một luồng vào mỗi lúc (~: chờ khóa); mỗi luồng tốn bộ",
                "  nhớ và thời gian tạo (+)."));
        NHAN_XET.put("nhom_luong", List.of(
                "Nhận xét: số luồng cố định nên tài nguyên được giới hạn; khi",
                "  2 thợ đều bận và hàng đợi đầy, yêu cầu bị từ chối ngay (503)",
                "  thay vì làm máy chủ quá tải."));
        NHAN_XET.put("bat_dong_bo", List.of(
                "Nhận xét: một luồng vẫn phục vụ nhiều yêu cầu vì I/O không",
                "  chặn (hàng I/O đang chờ có tới 6 lệnh cùng lúc); nhưng việc",
                "  CPU chạy trên vòng lặp làm mọi việc khác, kể cả nhịp tim,",
                "  phải chờ."));
        NHAN_XET.put("phoi_hop", List.of(
                "Nhận xét: vòng lặp chỉ điều phối (nhận, gửi lệnh I/O, trả lời);",
                "  việc CPU chạy trên nhóm luồng, song song trên 2 lõi, rồi báo",
                "  về vòng lặp: vòng lặp luôn rảnh, nhịp tim đúng giờ."));
    }

    /** Một luồng của máy chủ và các đoạn hoạt động của nó: [t0, t1, trạng thái, mã yêu cầu]. */
    static final class Lane {
        final String name;
        final List<Seg> segs = new ArrayList<>();
        final List<Long> marks = new ArrayList<>();   // thời điểm vòng lặp chạy một việc rất ngắn

        Lane(String name) { this.name = name; }

        void seg(long t0, long t1, String kind, String label) {
            if (t1 > t0) segs.add(new Seg(t0, t1, kind, label));
        }
    }

    record Seg(long t0, long t1, String kind, String label) {}

    /** Việc chờ một tài nguyên (lõi CPU hoặc khóa): luồng, mã yêu cầu, việc làm tiếp, lúc bắt đầu chờ. */
    record Cho(Lane lane, String label, Runnable done, long tw) {}

    /** Việc trong hàng đợi của vòng lặp sự kiện; dur > 0: việc CPU chạy ngay trên vòng lặp. */
    record Viec(Runnable fn, long dur, String label) {}

    /** Thông tin một yêu cầu: client, lúc gửi, lúc tới H5, lúc client nhận trả lời, mã trả lời. */
    static final class Req {
        final String client;
        final long gui;
        long den, nhan;
        int ma;
        Req(String client, long gui) { this.client = client; this.gui = gui; }
    }

    /** Chỉ số so sánh của một mô hình. */
    record ChiSo(long hoanTat, long tb, long lon, int luong, int tuChoi, long nhip) {}

    final Mang net;
    final String mode;
    final List<Lane> lanes = new ArrayList<>();
    int freeCore = SO_LOI;                                  // lõi CPU rảnh
    final ArrayDeque<Cho> cpuWait = new ArrayDeque<>();     // hàng chờ lõi (FIFO)
    boolean lockBusy;
    final ArrayDeque<Cho> lockWait = new ArrayDeque<>();
    int dem;                                                // bộ đếm chung (đoạn găng)
    final List<Object> io = new ArrayList<>();              // I/O bất đồng bộ: [t0, t1, mã]
    final List<long[]> beats = new ArrayList<>();           // nhịp tim [dự kiến, thực tế]
    final Map<String, Req> req = new HashMap<>();
    int choXuLy = YEU_CAU.size();                           // số yêu cầu máy chủ chưa trả lời
    int soLuong, maxLuong, tuChoi;

    // trạng thái riêng của từng mô hình
    Lane chinh, vl;
    final ArrayDeque<String> choKetNoi = new ArrayDeque<>();
    boolean ban;
    Lane[] tho;
    boolean[] ranh;
    final ArrayDeque<String[]> hang = new ArrayDeque<>();   // nhóm luồng: {mã, lúc vào hàng}; phối hợp: {mã}
    final ArrayDeque<Viec> ready = new ArrayDeque<>();
    boolean vlBan;

    MayChu(Mang net, String mode) {
        this.net = net;
        this.mode = mode;
    }

    void log(String text) { net.log(text, SERVER); }

    Lane lane(String name) {
        Lane l = new Lane(name);
        lanes.add(l);
        return l;
    }

    // ---------- tài nguyên dùng chung: lõi CPU và khóa ----------

    /** Chạy 10 ms CPU trên `lane`; hết lõi thì chờ theo thứ tự đến. */
    void cpu(Lane lane, String label, Runnable done) {
        if (freeCore > 0) {
            freeCore--;
            cpuStart(lane, label, done);
        } else {
            log(lane.name + ": " + label + " chờ lõi CPU (cả " + SO_LOI + " lõi đang bận)");
            cpuWait.add(new Cho(lane, label, done, net.now));
        }
    }

    void cpuStart(Lane lane, String label, Runnable done) {
        long t = net.now;
        lane.seg(t, t + CPU_US, "cpu", label);
        net.at(t + CPU_US, () -> cpuEnd(done));
    }

    void cpuEnd(Runnable done) {
        if (!cpuWait.isEmpty()) {                           // trao lõi vừa rảnh cho luồng chờ lâu nhất
            Cho c = cpuWait.poll();
            c.lane().seg(c.tw(), net.now, "cho_cpu", c.label());
            cpuStart(c.lane(), c.label(), c.done());
        } else {
            freeCore++;
        }
        done.run();
    }

    /** Vào đoạn găng: giữ khóa 3 ms để cập nhật bộ đếm chung; khóa đang bị giữ thì chờ. */
    void khoa(Lane lane, String label, Runnable done) {
        if (!lockBusy) {
            lockBusy = true;
            khoaGiu(lane, label, done);
        } else {
            log(lane.name + ": " + label + " chờ khóa (luồng khác đang trong đoạn găng)");
            lockWait.add(new Cho(lane, label, done, net.now));
        }
    }

    void khoaGiu(Lane lane, String label, Runnable done) {
        long t = net.now;
        lane.seg(t, t + KHOA_US, "khoa", label);
        net.at(t + KHOA_US, () -> {
            dem++;
            log(lane.name + ": " + label + " vào đoạn găng, bộ đếm chung = " + dem + ", nhả khóa");
            if (!lockWait.isEmpty()) {
                Cho c = lockWait.poll();
                c.lane().seg(c.tw(), net.now, "cho_khoa", c.label());
                khoaGiu(c.lane(), c.label(), c.done());
            } else {
                lockBusy = false;
            }
            done.run();
        });
    }

    // ---------- mạng: gửi yêu cầu, nhận yêu cầu, trả lời ----------
    void batDau() {
        net.onDeliver = this::nhan;
        for (YeuCau y : YEU_CAU) {
            req.put(y.label(), new Req(y.client(), y.t()));
            net.at(y.t(), () -> {
                net.log(y.client() + " gửi yêu cầu " + y.label() + " tới máy chủ " + SERVER, y.client());
                Mang.Packet p = new Mang.Packet("uni", "YC", y.client(), SERVER,
                        "YC " + y.label() + " " + y.client() + "→" + SERVER, 200);
                p.msg = y.label();                          // mã yêu cầu
                net.send(y.client(), p);
            });
        }
        switch (mode) {
            case "dong_bo" -> khoiTaoDongBo();
            case "da_luong" -> { }
            case "nhom_luong" -> khoiTaoNhomLuong();
            case "bat_dong_bo" -> khoiTaoBatDongBo();
            default -> khoiTaoPhoiHop();
        }
    }

    void nhan(String node, Mang.Packet pkt) {
        String label = pkt.msg;
        Req r = req.get(label);
        if (node.equals(SERVER)) {
            r.den = net.now;
            log("yêu cầu " + label + " từ " + r.client + " tới " + SERVER);
            switch (mode) {
                case "dong_bo" -> denDongBo(label);
                case "da_luong" -> denDaLuong(label);
                case "nhom_luong" -> denNhomLuong(label);
                case "bat_dong_bo" -> vlNhan(label, this::batIoXong);
                default -> vlNhan(label, this::phoiIoXong);
            }
        } else {
            r.nhan = net.now;
            r.ma = pkt.n;
            net.log(node + " nhận trả lời " + label + " (" + pkt.n + ") sau "
                    + Mang.fmtT(net.now - r.gui) + " ms", node);
        }
    }

    void traLoi(String label, int code) {
        String c = req.get(label).client;
        choXuLy--;
        String text = (code == 200 ? "TL " : "503 ") + label + " " + SERVER + "→" + c;
        Mang.Packet p = new Mang.Packet("uni", "TL", SERVER, c, text, code == 200 ? 1000 : 100);
        p.msg = label;
        p.n = code;                                         // mã trả lời: 200 hoặc 503
        net.send(SERVER, p);
    }

    void traLoi(String label) { traLoi(label, 200); }

    // ---------- 1. đồng bộ, một luồng ----------
    void khoiTaoDongBo() {
        chinh = lane("luồng chính");
        soLuong = maxLuong = 1;
    }

    void denDongBo(String label) {
        if (ban) {
            choKetNoi.add(label);
            log("luồng chính bận → " + label + " chờ trong hàng đợi kết nối (" + choKetNoi.size() + ")");
        } else {
            dongBoXuLy(label);
        }
    }

    void dongBoXuLy(String label) {
        ban = true;
        long t = net.now;
        log("luồng chính xử lý " + label + ": đọc CSDL, luồng bị chặn " + IO_US / MS + " ms");
        chinh.seg(t, t + IO_US, "io", label);
        net.at(t + IO_US, () -> cpu(chinh, label, () -> dongBoXong(label)));
    }

    void dongBoXong(String label) {
        log("luồng chính xong " + label + " (CPU " + CPU_US / MS + " ms), gửi trả lời");
        traLoi(label);
        if (!choKetNoi.isEmpty()) dongBoXuLy(choKetNoi.poll());
        else ban = false;
    }

    // ---------- 2. đa luồng: mỗi yêu cầu một luồng ----------
    void denDaLuong(String label) {
        Lane l = lane("T" + (lanes.size() + 1));
        soLuong++;
        maxLuong = Math.max(maxLuong, soLuong);
        log("tạo luồng " + l.name + " cho " + label + " (" + soLuong + " luồng đang chạy)");
        long t = net.now;
        l.seg(t, t + TAO_US, "tao", label);
        Runnable xong = () -> {
            soLuong--;
            log(l.name + ": gửi trả lời " + label + ", luồng kết thúc (" + soLuong + " luồng còn chạy)");
            traLoi(label);
        };
        Runnable doc = () -> {
            long u = net.now;
            log(l.name + ": " + label + " đọc CSDL (" + l.name + " bị chặn, các luồng khác vẫn chạy)");
            l.seg(u, u + IO_US, "io", label);
            net.at(u + IO_US, () -> cpu(l, label, () -> khoa(l, label, xong)));
        };
        net.at(t + TAO_US, doc);
    }

    // ---------- 3. nhóm luồng + hàng đợi giới hạn ----------
    void taoNhom(String tienTo) {
        tho = new Lane[SO_THO];
        ranh = new boolean[SO_THO];
        for (int i = 0; i < SO_THO; i++) {
            tho[i] = lane(tienTo + (i + 1));
            ranh[i] = true;
        }
    }

    void khoiTaoNhomLuong() {
        taoNhom("W");
        soLuong = maxLuong = SO_THO;
    }

    void denNhomLuong(String label) {
        for (int i = 0; i < SO_THO; i++) {
            if (ranh[i]) {
                nhomGiao(i, label);
                return;
            }
        }
        if (hang.size() < HANG_TOI_DA) {
            hang.add(new String[] {label, Long.toString(net.now)});
            log("W1, W2 đều bận → " + label + " vào hàng đợi (" + hang.size() + "/" + HANG_TOI_DA + ")");
        } else {
            tuChoi++;
            log("hàng đợi đầy → từ chối " + label + " ngay (503)");
            traLoi(label, 503);
        }
    }

    void nhomGiao(int i, String label) {
        ranh[i] = false;
        Lane l = tho[i];
        long t = net.now;
        log(l.name + " nhận " + label + ": đọc CSDL (" + l.name + " bị chặn " + IO_US / MS + " ms)");
        l.seg(t, t + IO_US, "io", label);
        net.at(t + IO_US, () -> cpu(l, label, () -> nhomXong(i, label)));
    }

    void nhomXong(int i, String label) {
        log("W" + (i + 1) + " xong " + label + ", gửi trả lời");
        traLoi(label);
        if (!hang.isEmpty()) {
            String[] h = hang.poll();
            long tw = Long.parseLong(h[1]);
            log("W" + (i + 1) + " lấy " + h[0] + " từ hàng đợi (đã chờ " + Mang.fmtT(net.now - tw) + " ms)");
            nhomGiao(i, h[0]);
        } else {
            ranh[i] = true;
        }
    }

    // ---------- vòng lặp sự kiện (dùng cho 4 và 5) ----------
    void khoiTaoVongLap() {
        vl = lane("vòng lặp");
        net.at(0, () -> nhip(0));
    }

    /** Đưa một việc vào hàng đợi của vòng lặp (dur > 0: việc CPU chạy ngay trên vòng lặp). */
    void post(Runnable fn, long dur, String label) {
        ready.add(new Viec(fn, dur, label));
        if (!vlBan) {
            vlBan = true;
            net.at(net.now, this::chay);
        }
    }

    void post(Runnable fn) { post(fn, 0, ""); }

    /** Vòng lặp lấy lần lượt từng việc; việc CPU giữ vòng lặp tới khi xong. */
    void chay() {
        while (!ready.isEmpty()) {
            Viec v = ready.poll();
            if (v.dur() != 0) {
                cpu(vl, v.label(), () -> {
                    v.fn().run();
                    chay();
                });
                return;
            }
            vl.marks.add(net.now);
            v.fn().run();
        }
        vlBan = false;
    }

    /** Nhịp tim mỗi 10 ms: một việc rất ngắn trên vòng lặp; đo nó chạy muộn bao nhiêu. */
    void nhip(long p) {
        if (choXuLy == 0) return;
        post(() -> {
            beats.add(new long[] {p, net.now});
            if (net.now - p >= MS)
                log("nhịp tim " + Mang.fmtT(p) + " ms chạy muộn " + Mang.fmtT(net.now - p) + " ms: vòng lặp bận");
        });
        net.at(p + NHIP_US, () -> nhip(p + NHIP_US));
    }

    void vlNhan(String label, java.util.function.Consumer<String> xongIo) {
        post(() -> {
            long t = net.now;
            log("vòng lặp nhận " + label + ", gửi lệnh đọc CSDL không chặn");
            io.add(List.of(t, t + IO_US, label));
            net.at(t + IO_US, () -> xongIo.accept(label));
        });
    }

    // ---------- 4. bất đồng bộ: một vòng lặp, CPU trên vòng lặp ----------
    void khoiTaoBatDongBo() {
        soLuong = maxLuong = 1;
        khoiTaoVongLap();
    }

    void batIoXong(String label) {
        log("CSDL xong " + label + " → việc CPU " + CPU_US / MS + " ms vào hàng đợi vòng lặp");
        post(() -> {
            log("vòng lặp tính xong " + label + " (CPU trên vòng lặp), trả lời");
            traLoi(label);
        }, CPU_US, label);
    }

    // ---------- 5. phối hợp: vòng lặp + nhóm luồng cho việc CPU ----------
    void khoiTaoPhoiHop() {
        soLuong = maxLuong = 1 + SO_THO;
        khoiTaoVongLap();
        taoNhom("P");
    }

    void phoiIoXong(String label) {
        post(() -> {
            log("vòng lặp giao việc CPU của " + label + " cho nhóm luồng");
            for (int i = 0; i < SO_THO; i++) {
                if (ranh[i]) {
                    phoiChay(i, label);
                    return;
                }
            }
            hang.add(new String[] {label});
            log("P1, P2 đều bận → việc CPU của " + label + " chờ trong hàng đợi");
        });
    }

    void phoiChay(int i, String label) {
        ranh[i] = false;
        cpu(tho[i], label, () -> phoiXong(i, label));
    }

    void phoiXong(int i, String label) {
        log("P" + (i + 1) + " tính xong " + label + " → gửi kết quả về vòng lặp");
        post(() -> {
            log("vòng lặp nhận kết quả " + label + " từ nhóm luồng, trả lời");
            traLoi(label);
        });
        if (!hang.isEmpty()) phoiChay(i, hang.poll()[0]);
        else ranh[i] = true;
    }

    // ---------- báo cáo ----------

    /** Căn phải chuỗi trong `w` ký tự (như str.rjust của Python). */
    static String rjust(String s, int w) { return s.length() >= w ? s : " ".repeat(w - s.length()) + s; }

    void gantt() {
        long end = 0;
        for (Lane l : lanes) for (Seg s : l.segs) end = Math.max(end, s.t1());
        for (Object o : io) end = Math.max(end, (Long) ((List<?>) o).get(1));
        long step = Math.max(MS, (end + 46 * MS - 1) / (46 * MS) * MS);      // tối đa 46 ô
        int width = (int) ((end + step - 1) / step);
        System.out.println("Bên trong máy chủ " + SERVER + ", 1 ô = " + step / MS + " ms:");
        System.out.println("  CHỮ HOA dùng CPU, chữ thường bị chặn chờ I/O, + tạo luồng,");
        System.out.println("  - chờ lõi CPU, # giữ khóa, ~ chờ khóa, · việc rất ngắn");
        for (Lane l : lanes) {
            StringBuilder row = new StringBuilder();
            for (int i = 0; i < width; i++) {
                long mid = i * step + step / 2;
                String ch = " ";
                for (Seg s : l.segs) {
                    if (s.t0() <= mid && mid < s.t1()) {
                        ch = switch (s.kind()) {
                            case "cpu" -> s.label();
                            case "io" -> s.label().toLowerCase();
                            default -> KY_HIEU.get(s.kind());
                        };
                        break;
                    }
                }
                if (ch.equals(" ")) {
                    for (long m : l.marks) {
                        if (i * step <= m && m < (i + 1) * step) {
                            ch = "·";
                            break;
                        }
                    }
                }
                row.append(ch);
            }
            System.out.printf("  %-12s|%s|%n", l.name, row);
        }
        if (!io.isEmpty()) {
            StringBuilder row = new StringBuilder();
            for (int i = 0; i < width; i++) {
                long mid = i * step + step / 2;
                int n = 0;
                for (Object o : io) {
                    List<?> s = (List<?>) o;
                    if ((Long) s.get(0) <= mid && mid < (Long) s.get(1)) n++;
                }
                row.append(n > 0 ? Integer.toString(Math.min(n, 9)) : " ");
            }
            System.out.printf("  %-12s|%s|%n", "I/O đang chờ", row);
        }
        String right = width * step / MS + " ms";
        System.out.printf("  %-12s 0%s%n", "", rjust(right, width - 1));
    }

    /** Chỉ số so sánh: hoàn tất, phản hồi trung bình/lớn nhất, số luồng, từ chối, nhịp tim trễ. */
    ChiSo chiSo() {
        long tong = 0, lon = 0, hoanTat = 0;
        for (YeuCau y : YEU_CAU) {
            Req r = req.get(y.label());
            long ph = r.nhan - r.gui;
            tong += ph;
            lon = Math.max(lon, ph);
            hoanTat = Math.max(hoanTat, r.nhan);
        }
        long nhip = -1;
        for (long[] b : beats) nhip = Math.max(nhip, b[1] - b[0]);
        return new ChiSo(hoanTat, tong / YEU_CAU.size(), lon, maxLuong, tuChoi, nhip);
    }

    void report() {
        System.out.println();
        gantt();
        System.out.println();
        System.out.printf("  %-3s %-6s %6s %7s %8s %9s  Kết quả%n", "Mã", "Client", "Gửi", "Tới H5", "Nhận TL", "Phản hồi");
        for (YeuCau y : YEU_CAU) {
            Req r = req.get(y.label());
            String kq = r.ma == 200 ? "200 OK" : "503 từ chối";
            System.out.printf("  %-3s %-6s %6s %7s %8s %9s  %s%n", y.label(), y.client(), Mang.fmtT(r.gui),
                    Mang.fmtT(r.den), Mang.fmtT(r.nhan), Mang.fmtT(r.nhan - r.gui), kq);
        }
        ChiSo s = chiSo();
        System.out.println("  Hoàn tất lúc " + Mang.fmtT(s.hoanTat()) + " ms; phản hồi TB " + Mang.fmtT(s.tb())
                + " ms, lớn nhất " + Mang.fmtT(s.lon()) + " ms");
        System.out.println("  Luồng tối đa " + s.luong() + "; từ chối " + s.tuChoi());
        if (!beats.isEmpty()) {
            long tong = 0;
            for (long[] b : beats) tong += b[1] - b[0];
            System.out.println("  Nhịp tim " + NHIP_US / MS + " ms: " + beats.size() + " nhịp, trễ lớn nhất "
                    + Mang.fmtT(s.nhip()) + " ms, TB " + Mang.fmtT(tong / beats.size()) + " ms");
        }
        System.out.println();
        for (String line : NHAN_XET.get(mode)) System.out.println(line);
    }

    /** Dữ liệu cho hoạt hình HTML: các luồng, I/O đang chờ, nhịp tim. */
    Map<String, Object> data() {
        List<Object> ls = new ArrayList<>();
        for (Lane l : lanes) {
            List<Object> segs = new ArrayList<>();
            for (Seg s : l.segs) segs.add(List.of(s.t0(), s.t1(), s.kind(), s.label()));
            ls.add(List.of(l.name, segs, new ArrayList<Object>(l.marks)));
        }
        Map<String, Object> d = new LinkedHashMap<>();
        d.put("lanes", ls);
        if (!io.isEmpty()) d.put("io", io);
        if (!beats.isEmpty()) {
            List<Object> bs = new ArrayList<>();
            for (long[] b : beats) bs.add(List.of(b[0], b[1]));
            d.put("beats", bs);
        }
        return d;
    }

    /** Chạy một mô hình máy chủ trên `net`, in nhật ký và báo cáo; trả về dữ liệu hoạt hình. */
    static Map<String, Object> chay(Mang net, String mode) {
        net.hopTrace = false;
        System.out.println("6 yêu cầu tới " + SERVER + ", mỗi yêu cầu: đọc CSDL " + IO_US / MS + " ms (chờ I/O)");
        System.out.println("  + xử lý " + CPU_US / MS + " ms CPU; máy chủ có " + SO_LOI + " lõi CPU");
        MayChu m = new MayChu(net, mode);
        m.batDau();
        net.run();
        m.report();
        return m.data();
    }

    /** Chạy một mô hình máy chủ không in gì (net.quiet); trả về chỉ số so sánh. */
    static ChiSo chiSo(Mang net, String mode) {
        net.hopTrace = false;
        net.quiet = true;
        MayChu m = new MayChu(net, mode);
        m.batDau();
        net.run();
        return m.chiSo();
    }

    /** In bảng so sánh năm mô hình từ chỉ số đã đo (theo thứ tự của TITLES). */
    static void soSanh(Map<String, ChiSo> ketQua) {
        System.out.println("So sánh năm mô hình máy chủ: cùng 6 yêu cầu, mỗi yêu cầu");
        System.out.println("  " + IO_US / MS + " ms I/O + " + CPU_US / MS + " ms CPU, máy chủ " + SO_LOI
                + " lõi. TB, Lớn nhất: thời gian");
        System.out.println("  phản hồi; 503: số yêu cầu bị từ chối; Nhịp tim: trễ lớn nhất.");
        System.out.printf("  %-11s %5s %9s %9s %9s %3s %9s%n", "Mô hình", "Luồng", "Hoàn tất", "TB", "Lớn nhất", "503", "Nhịp tim");
        for (Map.Entry<String, ChiSo> e : ketQua.entrySet()) {
            ChiSo s = e.getValue();
            String nhip = s.nhip() >= 0 ? Mang.fmtT(s.nhip()) + " ms" : "–";
            System.out.printf("  %-11s %5d %9s %9s %9s %3d %9s%n", TEN_NGAN.get(e.getKey()), s.luong(),
                    Mang.fmtT(s.hoanTat()) + " ms", Mang.fmtT(s.tb()) + " ms", Mang.fmtT(s.lon()) + " ms",
                    s.tuChoi(), nhip);
        }
        System.out.println();
        System.out.println("Ghi nhớ: luồng giúp chồng thời gian chờ I/O nhưng tốn tài nguyên");
        System.out.println("  và cần khóa khi dùng chung dữ liệu; bất đồng bộ phục vụ nhiều");
        System.out.println("  I/O bằng một luồng nhưng không được làm việc CPU trên vòng");
        System.out.println("  lặp; phối hợp vòng lặp + nhóm luồng có được cả hai ưu điểm.");
    }
}
