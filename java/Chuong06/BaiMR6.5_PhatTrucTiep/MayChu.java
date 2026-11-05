import java.io.BufferedInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.HashSet;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Map;
import java.util.Set;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.5 – Lõi máy chủ trung chuyển phát trực tiếp 1→n (kiểu SFU), dạng lớp.
 *
 * Dùng chung cho ServerPhat (dòng lệnh) và ServerGui (giao diện); không chạm giao diện, báo sự kiện bằng hàm
 * bao(text). batDau/dung được nhiều lần; mọi socket có thời gian chờ để dừng gọn.
 *
 * Kiến trúc SFU (Selective Forwarding Unit – RFC 7667 mục 3.7): máy chủ KHÔNG giải nén, KHÔNG trộn; mỗi datagram của
 * máy phát được NHÂN BẢN nguyên byte tới từng người xem (fan-out):
 *     băng thông ra ≈ n × băng thông vào; CPU chỉ tốn cho send() – khác MCU ở MR6.3.
 * Máy chủ chỉ đọc header (byte đầu, loại khung, số khung) để: đếm gói/byte, lọc theo đăng ký (âm thanh/video) và giữ
 * BỘ ĐỆM GOP: khung I mới nhất + mọi khung P sau nó (như GOP cache của SRS, nginx-rtmp). Người vào muộn nhận ngay cả
 * nhóm -> giải được hình tức thì, không phải chờ I kế tiếp.
 *
 * Luồng: một luồng nhận kết nối TCP, mỗi client một luồng đọc TCP, một luồng nhận UDP chuyển tiếp ngay (mọi gói
 * phương tiện đi ra từ CHỈ luồng này nên thứ tự gói được giữ). Trạng thái chung (danh sách client, bộ đệm GOP,
 * thống kê) được bảo vệ bằng một khóa `khoa`.
 */
final class MayChu {
    /** Một kết nối: máy phát (vai trò "phat") hoặc người xem ("xem"). */
    static final class Khach {
        final int ma;
        final String ten, vaiTro, ip;
        final Socket conn;
        final OutputStream ra;
        InetSocketAddress udp;                     // địa chỉ UDP đã đăng ký
        boolean amThanh = true, video = true;      // đăng ký nhận (người xem)
        final double lucChao = Chung.bayGio();
        double lucMoc = lucChao;                   // mốc đo khung I đầu: lúc vào, hoặc lúc bắt đầu phát nếu muộn hơn
        boolean canBoDem;                          // chờ luồng UDP gửi bộ đệm GOP cho riêng người này
        long goiRa, byteRa, demByte;               // demByte: cửa sổ 1 giây
        double kbps;
        Double khungDauMs;                         // từ lúc chào tới mảnh khung I đầu tiên gửi cho người này
        String nguonDau = "";                      // "bộ đệm" hoặc "trực tiếp"
        int pli;

        Khach(int ma, String ten, String vaiTro, Socket conn, String ip) throws IOException {
            this.ma = ma; this.ten = ten; this.vaiTro = vaiTro; this.conn = conn; this.ip = ip;
            ra = conn.getOutputStream();
        }

        void gui(Map<String, Object> obj) {
            try {
                synchronized (ra) { Chung.guiJson(ra, obj); }
            } catch (IOException ignored) { }
        }

        String nhan() { return amThanh && video ? "A+V" : amThanh ? "A" : video ? "V" : "-"; }

        TomTat tomTat() {
            return new TomTat(ma, ten, udp == null ? "–" : udp.getAddress().getHostAddress() + ":" + udp.getPort(), nhan(),
                    goiRa, byteRa, kbps, khungDauMs, nguonDau, pli, Chung.bayGio() - lucChao);
        }
    }

    record TomTat(int ma, String ten, String udp, String nhan, long goiRa, long byteRa, double kbps, Double khungDauMs,
                  String nguonDau, int pli, double giay) {
        TomTat voiKbps(double k) { return new TomTat(ma, ten, udp, nhan, goiRa, byteRa, k, khungDauMs, nguonDau, pli, giay); }
    }

    record AnhXemTruoc(int w, int h, byte[] rgb, long ts, long so) {}

    /** Ảnh chụp trạng thái cho dòng lệnh và giao diện. */
    static final class TrangThai {
        boolean dangPhat, boDemBat;
        String tenPhat;
        Map<String, Object> thongTin;
        int boDemGoi;
        long boDemByte;
        Long boDemSoI;
        double kbpsVao, kbpsVaoAm, kbpsVaoHinh, kbpsRa, heSo;
        long byteVao, byteRa, goiVao, goiRa;
        int pliChuyen, pliBo;
        List<TomTat> nguoiXem, daRoi;
        AnhXemTruoc anh;
    }

    final Consumer<String> bao;
    final Object khoa = new Object();
    volatile boolean chay;
    volatile boolean boDemBat = true;
    volatile boolean xemTruoc;                     // true: tự giải mã một bản để giao diện hiện ảnh xem trước
    private final List<Thread> luong = new ArrayList<>();
    private ServerSocket srv;
    private DatagramSocket udpSock;
    int congUdp;
    String diaChi = "";
    private int maMoi;

    Map<Integer, Khach> ds;
    Khach mayPhat;
    boolean dangPhat;
    Map<String, Object> thongTin, dongHo;
    List<byte[]> boDem;
    long boDemByte;
    Long boDemSoI;
    long byteVao, goiVao, byteRa, goiRa;
    long demVaoAm, demVaoHinh, demRa;
    double kbpsVaoAm, kbpsVaoHinh, kbpsRa;
    int pliChuyen, pliBo;
    double lucPli;
    int saiNguon;
    List<TomTat> daRoi;                            // tóm tắt người xem đã rời (bảng tổng kết)
    AnhXemTruoc anhXemTruoc;
    private Chung.GhepKhung ghep;
    private Chung.GiaiMaVideo giaiMa;

    MayChu(Consumer<String> bao) {
        this.bao = bao != null ? bao : t -> { };
        lamMoi();
    }

    private void lamMoi() {
        ds = new LinkedHashMap<>();
        mayPhat = null;
        dangPhat = false;
        thongTin = dongHo = null;
        boDem = new ArrayList<>();
        boDemByte = 0;
        boDemSoI = null;
        byteVao = goiVao = byteRa = goiRa = 0;
        demVaoAm = demVaoHinh = demRa = 0;
        kbpsVaoAm = kbpsVaoHinh = kbpsRa = 0;
        pliChuyen = pliBo = 0;
        lucPli = -1e9;
        saiNguon = 0;
        daRoi = new ArrayList<>();
        anhXemTruoc = null;
        ghep = new Chung.GhepKhung();
        giaiMa = new Chung.GiaiMaVideo();
    }

    // ------------------------------------------------------------ khởi động / dừng
    void batDau(String host, int congTcp, int congUdp) throws IOException {
        if (congUdp <= 0) congUdp = congTcp + 1;
        InetAddress dc = InetAddress.getByName(host);
        ServerSocket s = new ServerSocket();
        DatagramSocket u;
        try {
            s.setReuseAddress(true);
            s.bind(new InetSocketAddress(dc, congTcp));
            u = new DatagramSocket(null);
            try {
                u.setReceiveBufferSize(1 << 20);
                u.setSendBufferSize(1 << 20);
                u.bind(new InetSocketAddress(dc, congUdp));
            } catch (IOException e) {
                u.close();
                throw e;
            }
        } catch (IOException e) {
            s.close();
            throw e;
        }
        s.setSoTimeout(500);
        u.setSoTimeout(500);
        synchronized (khoa) { lamMoi(); }
        this.congUdp = congUdp;
        maMoi = 1;
        srv = s;
        udpSock = u;
        diaChi = host + ":" + congTcp;
        chay = true;
        luong.clear();
        luong.add(Thread.ofPlatform().daemon().name("nhan-ket-noi").start(() -> nhanKetNoi(s)));
        luong.add(Thread.ofPlatform().daemon().name("nhan-udp").start(() -> nhanUdp(u)));
        bao.accept("TCP " + host + ":" + congTcp + ", UDP " + congUdp + "; bộ đệm GOP: " + (boDemBat ? "bật" : "tắt"));
    }

    void dung() {
        if (!chay) return;
        chay = false;
        List<Khach> dsK;
        synchronized (khoa) { dsK = new ArrayList<>(ds.values()); }
        for (Khach k : dsK) {
            try {
                k.conn.shutdownInput();
                k.conn.shutdownOutput();
            } catch (IOException ignored) { }
        }
        for (Thread t : luong) {
            try { t.join(2000); } catch (InterruptedException ignored) { }
        }
        try { srv.close(); } catch (IOException ignored) { }
        udpSock.close();
        bao.accept("Server đã dừng");
    }

    // ------------------------------------------------------------ TCP điều khiển
    /** Gửi sự kiện cho mọi client (gọi khi đang giữ khóa). */
    private void suKien(String text) {
        bao.accept("=> " + text);
        for (Khach k : new ArrayList<>(ds.values())) k.gui(Json.obj("loai", "su_kien", "noi_dung", text));
    }

    private List<Khach> dsNguoiXem() {
        List<Khach> r = new ArrayList<>();
        for (Khach k : ds.values()) if (k.vaiTro.equals("xem")) r.add(k);
        return r;
    }

    /** Gửi máy phát danh sách người xem mỗi khi danh sách đổi (gọi khi giữ khóa). */
    private void baoNguoiXem() {
        if (mayPhat == null) return;
        List<Khach> xs = dsNguoiXem();
        xs.sort(Comparator.comparingInt(k -> k.ma));
        List<Object> ten = new ArrayList<>();
        for (Khach k : xs) ten.add(k.ten);
        mayPhat.gui(Json.obj("loai", "nguoi_xem", "so", (long) ten.size(), "ds", ten));
    }

    private void nhanKetNoi(ServerSocket s) {
        while (chay) {
            Socket conn;
            try {
                conn = s.accept();
            } catch (SocketTimeoutException e) {
                continue;
            } catch (IOException e) {
                break;
            }
            try {
                conn.setSoTimeout(0);
                conn.setTcpNoDelay(true);
            } catch (IOException ignored) { }
            Thread.ofPlatform().daemon().name("phuc-vu").start(() -> phucVu(conn));
        }
    }

    private void phucVu(Socket conn) {
        Khach k = null;
        try {
            InputStream tep = new BufferedInputStream(conn.getInputStream());
            String ip = conn.getInetAddress().getHostAddress();
            int congKhach = conn.getPort();
            Map<String, Object> tin = Chung.docDong(tep);
            if (tin == null || !"chao".equals(tin.get("loai"))
                    || !("phat".equals(tin.get("vai_tro")) || "xem".equals(tin.get("vai_tro")))) return;
            if (!Chung.tenHopLe(tin.get("ten"))) {
                Chung.guiJson(conn.getOutputStream(), Json.obj("loai", "loi", "ly_do", "tên không hợp lệ"));
                return;
            }
            String ten = (String) tin.get("ten"), vaiTro = (String) tin.get("vai_tro");
            synchronized (khoa) {
                if (vaiTro.equals("phat") && mayPhat != null) {
                    Chung.guiJson(conn.getOutputStream(), Json.obj("loai", "loi", "ly_do", "đã có máy phát"));
                    bao.accept("Từ chối máy phát thứ hai " + ten);
                    return;
                }
                k = new Khach(maMoi++, ten, vaiTro, conn, ip);
                ds.put(k.ma, k);
                k.gui(Json.obj("loai", "chao", "ma", (long) k.ma, "cong_udp", (long) congUdp, "phat", dangPhat));
                if (vaiTro.equals("phat")) {
                    mayPhat = k;
                    bao.accept("#" + k.ma + " " + k.ten + " (phát) vào từ " + ip + ":" + congKhach);
                    suKien(k.ten + " kết nối (máy phát)");
                    baoNguoiXem();
                } else {
                    if (thongTin != null) k.gui(thongTin);         // bản đã lưu cho người vào sau
                    if (dongHo != null) k.gui(dongHo);
                    bao.accept("#" + k.ma + " " + k.ten + " (xem) vào từ " + ip + ":" + congKhach);
                    suKien(k.ten + " vào xem");
                    baoNguoiXem();
                }
            }
            while (chay) {
                tin = Chung.docDong(tep);
                if (tin == null) break;
                synchronized (khoa) { xuLy(k, tin); }
            }
        } catch (IOException ignored) {
        } finally {
            if (k != null) synchronized (khoa) { roi(k, "rời"); }
            try { conn.close(); } catch (IOException ignored) { }
        }
    }

    private void roi(Khach k, String lyDo) {
        if (ds.remove(k.ma) == null) return;
        if (k == mayPhat) {
            if (dangPhat) ketThucPhat();
            mayPhat = null;
            suKien(k.ten + " (máy phát) thoát");
        } else {
            daRoi.add(k.tomTat());
            suKien(k.ten + " " + lyDo);
            baoNguoiXem();
        }
    }

    private void ketThucPhat() {
        dangPhat = false;
        thongTin = dongHo = null;
        boDem = new ArrayList<>();
        boDemByte = 0;
        boDemSoI = null;
        ghep = new Chung.GhepKhung();
        giaiMa = new Chung.GiaiMaVideo();
        for (Khach x : ds.values()) x.gui(Json.obj("loai", "ket_thuc"));
    }

    private void xuLy(Khach k, Map<String, Object> tin) {
        Object loai = tin.get("loai");
        if (k.vaiTro.equals("phat")) {
            if ("thong_tin".equals(loai)) {
                thongTin = tin;
                for (Khach x : dsNguoiXem()) x.gui(tin);            // chuyển tiếp nguyên văn
                bao.accept(k.ten + " phát " + Chung.chu(tin.get("rong")) + "x" + Chung.chu(tin.get("cao")) + " "
                        + Chung.chu(tin.get("fps")) + " fps GOP " + Chung.chu(tin.get("gop")) + ", "
                        + Chung.chu(tin.get("tan_so_mau")) + " Hz");
                if (!dangPhat) {
                    dangPhat = true;
                    double bayGio = Chung.bayGio();
                    for (Khach x : dsNguoiXem())                  // người vào trước lúc phát: đo từ lúc phát
                        if (x.khungDauMs == null) x.lucMoc = bayGio;
                    suKien(k.ten + " bắt đầu phát");
                }
            } else if ("dong_ho".equals(loai)) {
                dongHo = tin;
                for (Khach x : dsNguoiXem()) x.gui(tin);
            } else if ("dung_phat".equals(loai) && dangPhat) {
                ketThucPhat();
                suKien(k.ten + " dừng phát");
            }
            return;
        }
        if ("dang_ky".equals(loai)) {
            boolean coVideo = k.video;
            k.amThanh = Chung.dung(tin.getOrDefault("am_thanh", k.amThanh), k.amThanh);
            k.video = Chung.dung(tin.getOrDefault("video", k.video), k.video);
            if (k.video && !coVideo && boDemBat && k.udp != null) k.canBoDem = true;   // bật lại video: gửi bộ đệm GOP
            bao.accept(k.ten + " đăng ký nhận: " + k.nhan());
        } else if ("xin_khung_khoa".equals(loai)) {
            k.pli++;
            double bayGio = Chung.bayGio();
            if (mayPhat != null && dangPhat && bayGio - lucPli >= Chung.PLI_SERVER_S) {
                lucPli = bayGio;
                pliChuyen++;
                mayPhat.gui(Json.obj("loai", "xin_khung_khoa", "tu", k.ten));
                bao.accept("PLI từ " + k.ten + " -> " + mayPhat.ten);
            } else {
                pliBo++;
                bao.accept("PLI từ " + k.ten + ": bỏ (lọc 500 ms)");
            }
        }
    }

    // ------------------------------------------------------------ UDP: nhận và nhân bản
    private record Gui(byte[] goi, InetSocketAddress dich) {}

    private void nhanUdp(DatagramSocket sock) {
        double moc = Chung.bayGio();
        byte[] buf = new byte[2048];
        DatagramPacket dp = new DatagramPacket(buf, buf.length);
        while (chay) {
            byte[] goi = null;
            InetSocketAddress tu = null;
            try {
                dp.setLength(buf.length);
                sock.receive(dp);
                goi = java.util.Arrays.copyOf(buf, dp.getLength());
                tu = (InetSocketAddress) dp.getSocketAddress();
            } catch (SocketTimeoutException e) {
                goi = null;
            } catch (IOException e) {
                break;
            }
            double bayGio = Chung.bayGio();
            guiBoDemCho(sock);                                     // bộ đệm GOP đi TRƯỚC mọi gói trực tiếp
            if (goi != null && goi.length > 0) {
                List<Gui> ra;
                synchronized (khoa) { ra = xuLyGoi(goi, tu, bayGio); }
                for (Gui g : ra) {
                    try { sock.send(new DatagramPacket(g.goi(), g.goi().length, g.dich())); } catch (IOException ignored) { }
                }
            }
            if (bayGio - moc >= 1.0) {
                moc += 1.0;
                if (bayGio - moc > 1.0) moc = bayGio;
                synchronized (khoa) {
                    kbpsVaoAm = demVaoAm * 8 / 1000.0;
                    kbpsVaoHinh = demVaoHinh * 8 / 1000.0;
                    kbpsRa = demRa * 8 / 1000.0;
                    demVaoAm = demVaoHinh = demRa = 0;
                    for (Khach k : ds.values()) {
                        k.kbps = k.demByte * 8 / 1000.0;
                        k.demByte = 0;
                    }
                }
            }
        }
    }

    /** Gửi toàn bộ bộ đệm GOP cho người vừa có địa chỉ UDP / vừa bật lại video.
     *  Nghỉ 1 ms mỗi 32 gói để không làm tràn bộ đệm nhận của người xem. */
    private void guiBoDemCho(DatagramSocket sock) {
        List<Khach> can = new ArrayList<>();
        List<byte[]> goi;
        Long soI;
        synchronized (khoa) {
            for (Khach k : dsNguoiXem()) if (k.canBoDem && k.udp != null) can.add(k);
            if (can.isEmpty()) return;
            goi = boDemBat ? new ArrayList<>(boDem) : List.of();
            soI = boDemSoI;
            for (Khach k : can) k.canBoDem = false;
        }
        if (goi.isEmpty()) return;
        for (Khach k : can) {
            for (int n = 0; n < goi.size(); n++) {
                byte[] g = goi.get(n);
                try { sock.send(new DatagramPacket(g, g.length, k.udp)); } catch (IOException ignored) { }
                if (n % 32 == 31) Chung.ngu(0.001);
            }
            synchronized (khoa) {
                long b = 0;
                Set<Long> soP = new HashSet<>();
                for (byte[] g : goi) {
                    b += g.length;
                    if (g[1] == Chung.LOAI_P) soP.add(java.nio.ByteBuffer.wrap(g).getInt(4) & 0xFFFFFFFFL);
                }
                k.goiRa += goi.size();
                k.byteRa += b;
                k.demByte += b;
                goiRa += goi.size();
                byteRa += b;
                demRa += b;
                bao.accept(k.ten + ": gửi bộ đệm GOP " + goi.size() + " gói (I #" + soI + " + " + soP.size() + " P)");
                if (k.khungDauMs == null) {
                    k.khungDauMs = (Chung.bayGio() - k.lucMoc) * 1000;
                    k.nguonDau = "bộ đệm";
                    bao.accept(Chung.f("%s: khung I đầu sau %.0f ms (bộ đệm)", k.ten, k.khungDauMs));
                }
            }
        }
    }

    /** Xử lý một datagram (đang giữ khóa). Trả về danh sách (gói, đích) cần gửi. */
    private List<Gui> xuLyGoi(byte[] goi, InetSocketAddress tu, double bayGio) {
        int b0 = goi[0] & 0xFF;
        boolean video;
        Chung.Header h = null;
        if (b0 == Chung.PHIEN_BAN) {
            h = Chung.tachHeader(goi, goi.length);
            if (h == null) return List.of();
            if (h.loai() == Chung.LOAI_GIU) {
                Khach k = ds.get(h.ma());
                if (k != null && tu.getAddress().getHostAddress().equals(k.ip) && !tu.equals(k.udp)) {
                    boolean lanDau = k.udp == null;
                    k.udp = tu;
                    bao.accept("#" + k.ma + " " + k.ten + " đăng ký UDP " + tu.getAddress().getHostAddress() + ":" + tu.getPort());
                    if (lanDau && k.vaiTro.equals("xem") && k.video && boDemBat) k.canBoDem = true;
                }
                return List.of();
            }
            video = true;
        } else if (b0 == Chung.DAU_RTP) {
            video = false;
        } else {
            return List.of();
        }
        if (mayPhat == null || !tu.equals(mayPhat.udp) || !dangPhat) {
            saiNguon++;                                            // chỉ nhận phương tiện từ máy phát
            return List.of();
        }
        goiVao++;
        byteVao += goi.length;
        if (video) {
            demVaoHinh += goi.length;
            capNhatBoDem(h.loai(), h.so(), goi);
            if (xemTruoc) giaiMaXemTruoc(h, bayGio);
        } else {
            demVaoAm += goi.length;
        }
        List<Gui> ra = new ArrayList<>();
        for (Khach k : dsNguoiXem()) {
            if (k.udp == null || k.canBoDem || !(video ? k.video : k.amThanh)) continue;
            ra.add(new Gui(goi, k.udp));
            k.goiRa++;
            k.byteRa += goi.length;
            k.demByte += goi.length;
            if (video && h.loai() == Chung.LOAI_I && k.khungDauMs == null) {
                k.khungDauMs = (bayGio - k.lucMoc) * 1000;
                k.nguonDau = "trực tiếp";
                bao.accept(Chung.f("%s: khung I đầu sau %.0f ms (trực tiếp)", k.ten, k.khungDauMs));
            }
        }
        goiRa += ra.size();
        byteRa += (long) goi.length * ra.size();
        demRa += (long) goi.length * ra.size();
        return ra;
    }

    /** Bộ đệm GOP: mảnh đầu tiên của khung I mới -> xóa; lưu mảnh của I và các P sau nó. */
    private void capNhatBoDem(int loai, long so, byte[] goi) {
        if (!boDemBat) return;
        if (loai == Chung.LOAI_I && (boDemSoI == null || so != boDemSoI)) {
            boDem = new ArrayList<>();
            boDemByte = 0;
            boDemSoI = so;
        } else if (loai != Chung.LOAI_I && boDemSoI == null) {
            return;                                                // chưa có I (hoặc vừa tràn): chờ I kế tiếp
        }
        boDem.add(goi);
        boDemByte += goi.length;
        if (boDemByte > Chung.BO_DEM_GOP_TOI_DA) {
            boDem = new ArrayList<>();
            boDemByte = 0;
            boDemSoI = null;
            bao.accept("Bộ đệm GOP quá 4 MB: xóa, chờ khung I kế tiếp");
        }
    }

    /** Server giải mã MỘT bản chỉ để hiện ảnh xem trước (không ảnh hưởng việc chuyển tiếp). */
    private void giaiMaXemTruoc(Chung.Header h, double bayGio) {
        Chung.Khung k = ghep.them(h.loai(), h.so(), h.i(), h.tong(), h.w(), h.h(), h.ts(), h.payload(), bayGio);
        if (k == null) return;
        giaiMa.kiemTra(bayGio);
        for (Chung.KhungGiai g : giaiMa.them(k, bayGio)) anhXemTruoc = new AnhXemTruoc(g.w(), g.h(), g.anh(), g.ts(), g.so());
    }

    // ------------------------------------------------------------ lệnh của bảng điều khiển
    void datBoDem(boolean bat) {
        synchronized (khoa) {
            boDemBat = bat;
            if (!bat) {
                boDem = new ArrayList<>();
                boDemByte = 0;
                boDemSoI = null;
            }
        }
        bao.accept("Bộ đệm GOP: " + (bat ? "bật" : "tắt"));
    }

    /** Đuổi người xem theo tên (hoặc mã, ma ≥ 0). Trả về số người bị ngắt. */
    int ngat(String ten, int ma) {
        List<Khach> bi = new ArrayList<>();
        synchronized (khoa) {
            for (Khach k : dsNguoiXem()) if ((ma >= 0 && k.ma == ma) || k.ten.equals(ten)) bi.add(k);
            for (Khach k : bi) {
                k.gui(Json.obj("loai", "bi_ngat", "ly_do", "server ngắt"));
                roi(k, "bị server ngắt");
            }
        }
        for (Khach k : bi) {
            try {
                k.conn.shutdownInput();
                k.conn.shutdownOutput();
            } catch (IOException ignored) { }
        }
        return bi.size();
    }

    TrangThai trangThai() {
        synchronized (khoa) {
            TrangThai t = new TrangThai();
            double vao = kbpsVaoAm + kbpsVaoHinh;
            t.dangPhat = dangPhat;
            t.tenPhat = mayPhat != null ? mayPhat.ten : null;
            t.thongTin = thongTin;
            t.boDemBat = boDemBat;
            t.boDemGoi = boDem.size();
            t.boDemByte = boDemByte;
            t.boDemSoI = boDemSoI;
            t.kbpsVao = vao;
            t.kbpsVaoAm = kbpsVaoAm;
            t.kbpsVaoHinh = kbpsVaoHinh;
            t.kbpsRa = kbpsRa;
            t.heSo = vao > 0 ? kbpsRa / vao : 0.0;
            t.byteVao = byteVao;
            t.byteRa = byteRa;
            t.goiVao = goiVao;
            t.goiRa = goiRa;
            t.pliChuyen = pliChuyen;
            t.pliBo = pliBo;
            List<Khach> xs = dsNguoiXem();
            xs.sort(Comparator.comparingInt(k -> k.ma));
            t.nguoiXem = new ArrayList<>();
            for (Khach k : xs) t.nguoiXem.add(k.tomTat());
            t.daRoi = new ArrayList<>(daRoi);
            t.anh = anhXemTruoc;
            return t;
        }
    }

    // ------------------------------------------------------------ bảng in (dùng chung dòng lệnh)
    /** Một dòng của bảng người xem (≤ 55 ký tự) – dùng chung cho trang_thai và tổng kết. */
    static String dongBang(TomTat x) {
        String dau = x.khungDauMs() == null ? "–" : Chung.f("%4.0f ms %s", x.khungDauMs(), x.nguonDau());
        String ten = x.ten().length() > 6 ? x.ten().substring(0, 6) : x.ten();
        return Chung.f("%-6s %-4s %6d %6.0f %6.0f %s", ten, x.nhan(), x.goiRa(), x.byteRa() / 1024.0, x.kbps(), dau);
    }

    static final String TIEU_DE_BANG = Chung.f("%-6s %-4s %6s %6s %6s khung I đầu", "tên", "nhận", "gói ra", "KB ra", "kbit/s");

    /** Các dòng in cho lệnh trang_thai. */
    static List<String> bangTrangThai(TrangThai t) {
        List<String> r = new ArrayList<>(List.of(
                "Trạng thái: " + (t.dangPhat ? t.tenPhat + " đang phát" : "chưa phát") + ", " + t.nguoiXem.size() + " người xem",
                Chung.f("bộ đệm GOP %s: %d gói, %.0f KB", t.boDemBat ? "bật" : "tắt", t.boDemGoi, t.boDemByte / 1024.0),
                Chung.f("vào %.0f kbit/s (tiếng %.0f, hình %.0f)", t.kbpsVao, t.kbpsVaoAm, t.kbpsVaoHinh),
                Chung.f("ra %.0f kbit/s, hệ số nhân bản %.2f", t.kbpsRa, t.heSo),
                "PLI chuyển " + t.pliChuyen + ", bỏ " + t.pliBo,
                TIEU_DE_BANG));
        for (TomTat x : t.nguoiXem) r.add(dongBang(x));
        return r;
    }

    /** Các dòng in khi server kết thúc: mọi người xem đã vào (cả người đã rời). */
    static List<String> bangTongKet(TrangThai t) {
        List<TomTat> xs = new ArrayList<>(t.daRoi);
        xs.addAll(t.nguoiXem);
        double heSo = t.byteVao > 0 ? (double) t.byteRa / t.byteVao : 0.0;
        List<String> r = new ArrayList<>(List.of(
                "===== TỔNG KẾT: " + xs.size() + " người xem =====",
                Chung.f("vào %d gói %.0f KB, ra %d gói %.0f KB", t.goiVao, t.byteVao / 1024.0, t.goiRa, t.byteRa / 1024.0),
                Chung.f("hệ số nhân bản (byte ra/vào) %.2f", heSo),
                "PLI chuyển " + t.pliChuyen + ", bỏ " + t.pliBo,
                TIEU_DE_BANG));
        for (TomTat x : xs) r.add(dongBang(x.voiKbps(x.byteRa() * 8 / 1000.0 / Math.max(x.giay(), 0.001))));
        r.add("(kbit/s = trung bình suốt thời gian xem)");
        return r;
    }
}
