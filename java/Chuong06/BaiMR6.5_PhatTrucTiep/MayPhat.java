import java.io.BufferedInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.util.ArrayList;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.5 – Lõi máy phát (camera + micro -> máy chủ), dạng lớp, không chạm giao diện.
 *
 * Dùng chung cho Phat (dòng lệnh) và PhatGui. Báo sự kiện bằng bao(text); giao diện đọc số liệu (xem trước, mức
 * micro, thống kê, danh sách người xem) qua trangThai() dưới khóa.
 *
 * Đồng hồ phương tiện chung: t = 0 khi bấm Phát (đồng hồ đơn điệu System.nanoTime).
 *   - Âm thanh: luồng 20 ms; gói thứ n được gửi lúc t0 + n·20 ms (bám đồng hồ, không trôi), nhãn RTP ts = n·320
 *     (= số mẫu đã gửi, 16 kHz).
 *   - Video: luồng theo fps; nhãn = ms phương tiện lúc chụp khung. Khung 0 là I; I lặp lại mỗi `gop` khung hoặc NGAY
 *     khung kế tiếp sau khi nhận xin_khung_khoa (đếm GOP lại từ khung I đó).
 *   - Mỗi 1 s gửi {"loai":"dong_ho","ts":ms phương tiện,"gio":ms Unix} (như RTCP Sender Report – RFC 3550 mục 6.4.1):
 *     người xem ánh xạ nhãn thời gian <-> giờ thực để đo độ trễ đầu-cuối.
 */
final class MayPhat {
    record XemTruoc(int w, int h, byte[] rgb, long ts) {}

    /** Ảnh chụp trạng thái (đọc dưới khóa). */
    static final class TrangThai {
        boolean dangPhat, daKetNoi;
        Integer ma;
        int soI, soP, pMin, pMax, pliNhan, fps, gop;
        long goiAm;
        double tbI, tbP, kbpsAm, kbpsHinh, mucMic;
        List<String> nguoiXem;
        XemTruoc xemTruoc;
        String camera, mic;
    }

    record GiayQua(int i, int p, double kbpsAm, double kbpsHinh) {}

    final String ten;
    final Consumer<String> bao;
    final Object khoa = new Object(), khoaTcp = new Object();
    volatile Socket tcp;
    volatile DatagramSocket udp;
    volatile boolean daKetNoi;
    volatile Integer ma;
    volatile boolean dangPhat;
    Camera.NguonCamera camera;
    ThietBiAmThanh.NguonAm mic;
    int fps = Chung.FPS_MAC_DINH, gop = Chung.GOP_MAC_DINH;
    List<String> nguoiXem = new ArrayList<>();        // tên người xem (theo thông điệp nguoi_xem)
    XemTruoc xemTruoc;                                // khung vừa gửi
    volatile double mucMic = Double.NEGATIVE_INFINITY;
    final double matGoi;
    final Random rng = new Random(1);
    volatile int boGiaLap;
    double t0;
    long so;
    int seq;
    long nAm;
    Long soICuoi;
    byte[] truoc;
    boolean canI;
    String lyDoI = "";
    int soI, soP, pliNhan;
    long byteI, byteP, goiAm;
    Integer pMin, pMax;
    int demI, demP;
    long demByteAm, demByteHinh;
    double kbpsAm, kbpsHinh;
    volatile boolean song = true;

    MayPhat(String ten, Consumer<String> bao, double matGoi) {
        this.ten = ten;
        this.bao = bao != null ? bao : t -> { };
        this.matGoi = matGoi;
        lamMoiThongKe();
        Chung.chayNen("phat-am", this::vongAm);
        Chung.chayNen("phat-hinh", this::vongHinh);
    }

    private void lamMoiThongKe() {
        soI = soP = pliNhan = 0;
        byteI = byteP = goiAm = 0;
        pMin = pMax = null;
        demI = demP = 0;
        demByteAm = demByteHinh = 0;
        kbpsAm = kbpsHinh = 0;
    }

    // ------------------------------------------------------------ kết nối
    /** Kết nối TCP, chào với vai trò "phat". Lỗi (kể cả server từ chối) -> IOException. */
    int ketNoi(String host, int cong) throws IOException {
        Socket s = new Socket();
        DatagramSocket u;
        Map<String, Object> tl;
        InputStream tep;
        try {
            s.connect(new InetSocketAddress(host, cong), 5000);
            s.setSoTimeout(5000);
            s.setTcpNoDelay(true);
            tep = new BufferedInputStream(s.getInputStream());
            Chung.guiJson(s.getOutputStream(), Json.obj("loai", "chao", "vai_tro", "phat", "ten", ten));
            tl = Chung.docDong(tep);
            if (tl == null) tl = Map.of();
            if ("loi".equals(tl.get("loai"))) throw new IOException("server từ chối: " + tl.get("ly_do"));
            if (!"chao".equals(tl.get("loai"))) throw new IOException("server không trả lời 'chao'");
            s.setSoTimeout(0);
            u = new DatagramSocket();
            u.setSendBufferSize(1 << 20);
            u.connect(new InetSocketAddress(host, (int) Chung.so(tl.get("cong_udp"), cong + 1)));
        } catch (IOException | RuntimeException e) {
            s.close();
            throw new IOException(e.getMessage(), e);
        }
        int m = (int) Chung.so(tl.get("ma"), 0);
        synchronized (khoa) {
            tcp = s;
            udp = u;
            ma = m;
            daKetNoi = true;
        }
        byte[] g = Chung.goiGiu(m);
        u.send(new DatagramPacket(g, g.length));                    // đăng ký địa chỉ UDP ngay
        Chung.chayNen("phat-tcp", () -> docTcp(tep));
        return m;
    }

    void gui(Map<String, Object> obj) {
        Socket s = tcp;
        if (s == null) return;
        try {
            OutputStream o = s.getOutputStream();
            synchronized (khoaTcp) { Chung.guiJson(o, obj); }
        } catch (IOException ignored) { }
    }

    private void docTcp(InputStream tep) {
        try {
            while (true) {
                Map<String, Object> tin = Chung.docDong(tep);
                if (tin == null) break;
                Object loai = tin.get("loai");
                if ("xin_khung_khoa".equals(loai)) {
                    synchronized (khoa) {
                        pliNhan++;
                        lyDoI = "PLI từ " + (tin.get("tu") == null ? "?" : Chung.chu(tin.get("tu")));
                        canI = true;
                    }
                } else if ("nguoi_xem".equals(loai)) {
                    List<String> d = new ArrayList<>();
                    if (tin.get("ds") instanceof List<?> l) for (Object o : l) d.add(Chung.chu(o));
                    synchronized (khoa) { nguoiXem = d; }
                } else if ("su_kien".equals(loai)) {
                    bao.accept("Sự kiện: " + Chung.chu(tin.get("noi_dung")));
                } else if ("loi".equals(loai)) {
                    bao.accept("Lỗi từ server: " + Chung.chu(tin.get("ly_do")));
                }
            }
        } catch (IOException ignored) { }
        if (daKetNoi) {
            bao.accept("Mất kết nối server");
            ngat();
        }
    }

    void ngat() {
        Socket s;
        DatagramSocket u;
        synchronized (khoa) {
            s = tcp;
            u = udp;
            tcp = null;
            udp = null;
            daKetNoi = false;
            dangPhat = false;
            nguoiXem = new ArrayList<>();
        }
        if (s != null) {
            try { s.shutdownInput(); s.shutdownOutput(); } catch (IOException ignored) { }
            try { s.close(); } catch (IOException ignored) { }
        }
        if (u != null) u.close();
    }

    // ------------------------------------------------------------ phát / dừng
    private Map<String, Object> thongTin() {
        return Json.obj("loai", "thong_tin", "rong", (long) Chung.RONG, "cao", (long) Chung.CAO, "fps", (long) fps,
                "gop", (long) gop, "tan_so_mau", (long) Chung.TAN_SO_MAU);
    }

    void batDauPhat(Camera.NguonCamera cam, ThietBiAmThanh.NguonAm m, int f, int g) {
        Map<String, Object> tt;
        synchronized (khoa) {
            camera = cam;
            mic = m;
            fps = Math.max(1, Math.min(30, f > 0 ? f : fps));
            gop = Math.max(1, Math.min(300, g > 0 ? g : gop));
            tt = thongTin();
        }
        // thong_tin đi TCP, gói phương tiện đi UDP trên luồng khác: gửi thong_tin trước và chờ 20 ms để máy chủ kịp
        // chuyển sang "đang phát" – nếu không, khung I #0 có thể tới trước, bị bỏ vì "chưa phát", và bộ đệm GOP rỗng
        // tới khung I kế tiếp.
        gui(tt);
        Chung.ngu(0.02);
        synchronized (khoa) {
            t0 = Chung.bayGio();
            so = 0;
            seq = 0;
            nAm = 0;
            soICuoi = null;
            truoc = null;
            canI = false;
            lyDoI = "";
            lamMoiThongKe();
            dangPhat = true;
            cam.datFps(fps);
        }
        bao.accept("Bắt đầu phát " + Chung.RONG + "x" + Chung.CAO + " " + fps + " fps GOP " + gop);
    }

    void datThamSo(int f, int g) {
        boolean phat;
        Map<String, Object> tt;
        synchronized (khoa) {
            fps = Math.max(1, Math.min(30, f));
            gop = Math.max(1, Math.min(300, g));
            if (camera != null) camera.datFps(fps);
            phat = dangPhat;
            tt = thongTin();
        }
        if (phat) gui(tt);
        bao.accept("Tham số: " + fps + " fps, GOP " + gop);
    }

    void khungKhoaNgay(String lyDo) {
        synchronized (khoa) {
            canI = true;
            lyDoI = lyDo;
        }
    }

    void dungPhat() {
        Camera.NguonCamera cam;
        ThietBiAmThanh.NguonAm m;
        synchronized (khoa) {
            if (!dangPhat) return;
            dangPhat = false;
            cam = camera;
            m = mic;
            camera = null;
            mic = null;
        }
        gui(Json.obj("loai", "dung_phat"));
        if (cam != null) cam.dong();
        if (m != null) m.dong();
        bao.accept("Dừng phát");
    }

    /** Gửi một datagram; matGoi giả lập mạng làm rơi gói ở chiều lên. */
    private void guiUdp(byte[] goi) {
        if (matGoi > 0 && rng.nextDouble() < matGoi) {
            boGiaLap++;
            return;
        }
        DatagramSocket u = udp;
        if (u != null) {
            try { u.send(new DatagramPacket(goi, goi.length)); } catch (IOException ignored) { }
        }
    }

    // ------------------------------------------------------------ luồng âm thanh 20 ms
    private void vongAm() {
        double hanGiu = Chung.bayGio();
        while (song) {
            double bayGio = Chung.bayGio();
            boolean phat;
            ThietBiAmThanh.NguonAm m;
            Integer mMa;
            DatagramSocket u;
            double t;
            synchronized (khoa) {
                phat = dangPhat;
                m = mic;
                mMa = ma;
                u = udp;
                t = t0;
            }
            if (u != null && bayGio >= hanGiu) {
                guiUdp(Chung.goiGiu(mMa));                          // giữ kết nối mỗi 1 s
                hanGiu = bayGio + 1.0;
            }
            if (!(phat && m != null && u != null)) {
                mucMic = Double.NEGATIVE_INFINITY;
                Chung.ngu(0.02);
                continue;
            }
            double han = t + nAm * Chung.KHUNG_MS / 1000.0;
            if (bayGio < han) {
                Chung.ngu(Math.min(han - bayGio, 0.02));
                continue;
            }
            if (nAm % 50 == 0)                                      // báo cáo đồng hồ mỗi 1 s
                gui(Json.obj("loai", "dong_ho", "ts", (long) ((bayGio - t) * 1000), "gio", Chung.gioUnixMs()));
            short[] khung = m.layKhung();
            mucMic = Chung.mucDbfs(khung);
            byte[] goi = Chung.dongGoiRtp(seq, nAm * Chung.MAU_MOI_KHUNG, mMa, Chung.pcmSangByte(khung));
            guiUdp(goi);
            synchronized (khoa) {
                seq = (seq + 1) & 0xFFFF;
                nAm++;
                goiAm++;
                demByteAm += goi.length;
            }
        }
    }

    // ------------------------------------------------------------ luồng video theo fps
    private void vongHinh() {
        Double han = null;
        while (song) {
            double bayGio = Chung.bayGio();
            boolean phat;
            Camera.NguonCamera cam;
            Integer mMa;
            DatagramSocket u;
            double t;
            int f;
            synchronized (khoa) {
                phat = dangPhat;
                cam = camera;
                mMa = ma;
                u = udp;
                t = t0;
                f = fps;
            }
            if (!(phat && cam != null && u != null)) {
                han = null;
                Chung.ngu(0.01);
                continue;
            }
            if (han == null || han < t) han = t;
            if (bayGio < han) {
                Chung.ngu(Math.min(han - bayGio, 0.01));
                continue;
            }
            han += 1.0 / f;
            if (han < bayGio - 0.2) han = bayGio;                   // tụt quá xa (máy bận): bắt nhịp lại
            String loi = cam.loi();
            if (loi != null && !loi.isEmpty()) {                     // ffmpeg dừng/lỗi: lùi về giả lập
                bao.accept("Camera lỗi, dùng giả lập: " + (loi.length() > 28 ? loi.substring(0, 28) : loi));
                cam.dong();
                cam = new Camera.GiaLap(ten, mMa != null ? mMa : 1, f);
                synchronized (khoa) { camera = cam; }
            }
            long tsMs = (long) ((bayGio - t) * 1000);
            byte[] anh = cam.layKhung(so, tsMs);
            if (anh == null || anh.length != Chung.RONG * Chung.CAO * 3) continue;   // camera chưa có khung
            long s;
            boolean laI;
            String lyDo;
            synchronized (khoa) {
                s = so;
                laI = soICuoi == null || s - soICuoi >= gop || canI;
                lyDo = canI ? lyDoI : "";
                canI = false;
                lyDoI = "";
            }
            int loai = laI ? Chung.LOAI_I : Chung.LOAI_P;
            byte[] duLieu = laI ? Chung.maHoaI(anh) : Chung.maHoaP(anh, truoc);
            List<byte[]> manh = Chung.chiaManh(loai, mMa, s, Chung.RONG, Chung.CAO, tsMs, duLieu);
            for (byte[] g : manh) guiUdp(g);
            if (!lyDo.isEmpty()) bao.accept(lyDo + " -> gửi khung I #" + s);
            synchronized (khoa) {
                truoc = anh;
                so++;
                if (laI) {
                    soICuoi = s;
                    soI++;
                    byteI += duLieu.length;
                    demI++;
                } else {
                    soP++;
                    byteP += duLieu.length;
                    pMin = pMin == null ? duLieu.length : Math.min(pMin, duLieu.length);
                    pMax = pMax == null ? duLieu.length : Math.max(pMax, duLieu.length);
                    demP++;
                }
                for (byte[] g : manh) demByteHinh += g.length;
                xemTruoc = new XemTruoc(Chung.RONG, Chung.CAO, truoc, tsMs);
            }
        }
    }

    // ------------------------------------------------------------ số liệu
    /** Gọi mỗi 1 s (dòng lệnh hoặc giao diện): trả về và xóa bộ đếm của giây vừa qua. */
    GiayQua thongKeGiay() {
        synchronized (khoa) {
            GiayQua r = new GiayQua(demI, demP, demByteAm * 8 / 1000.0, demByteHinh * 8 / 1000.0);
            kbpsAm = r.kbpsAm();
            kbpsHinh = r.kbpsHinh();
            demI = demP = 0;
            demByteAm = demByteHinh = 0;
            return r;
        }
    }

    TrangThai trangThai() {
        synchronized (khoa) {
            TrangThai t = new TrangThai();
            t.dangPhat = dangPhat;
            t.daKetNoi = daKetNoi;
            t.ma = ma;
            t.soI = soI;
            t.soP = soP;
            t.tbI = soI > 0 ? (double) byteI / soI : 0.0;
            t.tbP = soP > 0 ? (double) byteP / soP : 0.0;
            t.pMin = pMin == null ? 0 : pMin;
            t.pMax = pMax == null ? 0 : pMax;
            t.goiAm = goiAm;
            t.pliNhan = pliNhan;
            t.kbpsAm = kbpsAm;
            t.kbpsHinh = kbpsHinh;
            t.nguoiXem = new ArrayList<>(nguoiXem);
            t.mucMic = mucMic;
            t.xemTruoc = xemTruoc;
            t.fps = fps;
            t.gop = gop;
            t.camera = camera != null ? camera.moTa() : "";
            t.mic = mic != null ? mic.moTa() : "";
            return t;
        }
    }

    void dong() {
        dungPhat();
        song = false;
        ngat();
    }

    /** Các dòng tổng kết của máy phát (dùng chung cho dòng lệnh). */
    static List<String> dongTongKet(TrangThai t, double giay) {
        return List.of(
                Chung.f("===== TỔNG KẾT sau %.1f s =====", giay),
                Chung.f("khung I %d, P %d; TB I %.0f B, P %.0f B", t.soI, t.soP, t.tbI, t.tbP),
                Chung.f("P nhỏ nhất %d B, lớn nhất %d B (khung chớp)", t.pMin, t.pMax),
                Chung.f("I/thô %.1f %%, P TB/I TB %.1f %%", t.tbI / (Chung.RONG * Chung.CAO * 3) * 100,
                        t.tbI > 0 ? t.tbP / t.tbI * 100 : 0.0),
                "tiếng " + t.goiAm + " gói; PLI nhận " + t.pliNhan);
    }
}
