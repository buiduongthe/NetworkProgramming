import java.io.BufferedInputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.util.ArrayDeque;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.Iterator;
import java.util.List;
import java.util.Map;
import java.util.Random;
import java.util.function.Consumer;

/**
 * Bài mở rộng MR6.5 – Lõi người xem phát trực tiếp, dạng lớp, không chạm giao diện.
 *
 * Dùng chung cho Xem (dòng lệnh) và XemGui. Bốn luồng, chung một khóa `khoa`:
 *   1. luồng đọc TCP: thong_tin, dong_ho (báo cáo đồng hồ), su_kien, ket_thuc, bi_ngat;
 *   2. luồng nhận UDP: RTP -> bộ đệm jitter; mảnh video -> GhepKhung -> GiaiMaVideo -> hàng hiển thị;
 *   3. luồng phát 20 ms = ĐỒNG HỒ CHỦ: lấy một khung âm thanh, đưa ra loa, cập nhật dongHoAm = ts/16 (ms phương
 *      tiện) và giờ thực lúc phát; ghi lịch sử ~3 s để đo lệch;
 *   4. luồng hiển thị 5 ms: kiểm tra mất tham chiếu (xin khung khóa), chọn khung để hiện.
 *
 * Đồng bộ tiếng-hình (lip-sync) – âm thanh làm đồng hồ chủ (như trình phát thật: tai nhạy với tiếng bị ngắt hơn mắt
 * nhạy với một khung hình hiện sớm/muộn):
 *   - BẬT: chỉ hiện khung mới nhất có nhãn ≤ đồng hồ phát hiện tại dongHoAm + (bây giờ − lúc phát khung âm thanh
 *     đó); khung quá cũ bị bỏ.
 *   - TẮT (hoặc chỉ nhận video): hiện ngay khi giải mã xong -> hình đi TRƯỚC tiếng đúng bằng thời gian âm thanh nằm
 *     trong bộ đệm jitter (độ sâu × 20 ms).
 * Lệch tiếng-hình của một khung = giờ thực lúc hiện − giờ thực lúc âm thanh cùng nhãn được phát (nội suy trên lưới
 * 20 ms của lịch sử phát); dương = hình chậm hơn tiếng, âm = hình đi trước.
 * Độ trễ đầu-cuối (âm thanh) = giờ Unix lúc phát − (gio + (ts_ms − ts)) theo báo cáo đồng hồ mới nhất – chỉ đúng khi
 * máy phát và người xem cùng đồng hồ (cùng máy hoặc đồng bộ NTP).
 */
final class NguoiXem {
    /** Khung đang hiện: (w, h, rgb, ts, số khung, đồng hồ tiếng lúc hiện | null). */
    record Hien(int w, int h, byte[] rgb, long ts, long so, Double am) {}

    /** Ảnh chụp: (w, h, rgb, nhãn hình ms, đồng hồ tiếng LÚC HIỆN khung ms|null, lệch ms|null, số khung). */
    record Chup(int w, int h, byte[] rgb, long ts, Double am, Double lech, long so) {}

    static final class GiaiDoan {
        final boolean dongBo;
        double tong;
        int n;
        GiaiDoan(boolean dongBo) { this.dongBo = dongBo; }
        GiaiDoan sao() { GiaiDoan g = new GiaiDoan(dongBo); g.tong = tong; g.n = n; return g; }
    }

    record GiayQua(int am, long mat, int hien, Double tre, Double lech, double kbps) {}

    static final class TrangThai {
        boolean daKetNoi, dongBo, amThanh, video;
        Integer ma;
        Map<String, Object> thongTin;
        long goiAm, matAm;
        Double treTb, treHien, lechHien, khungDauMs;
        int khungI, khungP, khungHien, hong, boP, boCu, dungHinh, pliGui, doSau, boGiaLap;
        long soHien;
        String nguonDau;
        List<GiaiDoan> giaiDoan;
        double kbps, mucAm;
        Hien hien;
    }

    /** Loa: đối tượng có phat(mẫu) hoặc null (không phát). */
    interface Loa {
        void phat(short[] mau);
        void dong();
    }

    final String ten;
    final Consumer<String> bao;
    final Object khoa = new Object(), khoaTcp = new Object();
    volatile Socket tcp;
    volatile DatagramSocket udp;
    volatile boolean daKetNoi;
    volatile Integer ma;
    int doSau;
    volatile boolean dongBo, amThanh, video;
    final double matGoi;
    Random rng = new Random(0);
    volatile int boGiaLap;
    volatile Loa loa;
    Map<String, Object> thongTin;
    volatile double mucAm = Double.NEGATIVE_INFINITY;
    final List<GiaiDoan> giaiDoan = new ArrayList<>();

    // thống kê (cả buổi xem)
    double lucChao;
    double lucMoc;                                           // mốc đo khung đầu: lúc vào, hoặc lúc máy phát
    boolean choPhat;                                         // bắt đầu phát nếu khi vào chưa có ai phát
    long goiAm, byteNhan;
    int khungI, khungP, khungHien, dungHinh, pliGui, boCu;
    long matCu;
    int hongCu, boPCu;                                       // số đếm của các phiên phát trước
    double tongTre;
    int nTre;
    Double treHien, lechHien;
    Double khungDauMs;
    String nguonDau = "";
    Double[] iDau;                                           // (I cũ bao nhiêu ms | null, tới sau bao nhiêu ms)
    Double choHinhLai;
    double lucPli;
    boolean dangDung;
    int wAm, wHien;
    long wByte;
    List<Double> wTre = new ArrayList<>(), wLech = new ArrayList<>();

    // trạng thái một phiên phát (làm mới khi máy phát dừng phát)
    Chung.BoDemJitter boDem;
    Chung.GhepKhung ghep;
    Chung.GiaiMaVideo giaiMa;
    ArrayDeque<Chung.KhungGiai> hang;
    ArrayDeque<double[]> lichSu;                             // (ts ms, giờ đơn điệu) ~3 s gần nhất (150 phần tử)
    Double dongHoAm, lucPhatAm, lucNhanAm;
    Long tsAmMoi, tsHinhMoi;
    double[] dongHoBao;                                      // (ts, gio) báo cáo đồng hồ mới nhất
    Hien hien;
    long soHien;
    volatile boolean song = true;

    NguoiXem(String ten, Consumer<String> bao, int doSau, boolean dongBo, boolean amThanh, boolean video, double matGoi, Loa loa) {
        this.ten = ten;
        this.bao = bao != null ? bao : t -> { };
        this.doSau = doSau;
        this.dongBo = dongBo;
        this.amThanh = amThanh;
        this.video = video;
        this.matGoi = matGoi;
        this.loa = loa;
        giaiDoan.add(new GiaiDoan(dongBo));
        lamMoiThongKe();
        lamMoiPhien();
        Chung.chayNen("xem-am", this::vongAm);
        Chung.chayNen("xem-hien", this::vongHien);
    }

    private void lamMoiThongKe() {
        lucChao = Chung.bayGio();
        lucMoc = lucChao;
        choPhat = false;
        goiAm = byteNhan = 0;
        khungI = khungP = khungHien = 0;
        dungHinh = pliGui = boCu = 0;
        matCu = 0;
        hongCu = boPCu = 0;
        tongTre = 0;
        nTre = 0;
        treHien = lechHien = null;
        khungDauMs = null;
        nguonDau = "";
        iDau = null;
        choHinhLai = null;
        lucPli = -1e9;
        dangDung = false;
        wAm = wHien = 0;
        wByte = 0;
        wTre = new ArrayList<>();
        wLech = new ArrayList<>();
    }

    /** Trạng thái của một phiên phát (làm mới khi máy phát dừng phát). */
    private void lamMoiPhien() {
        if (boDem != null) {
            matCu += boDem.mat();
            hongCu += ghep.khungHong;
            boPCu += giaiMa.boP;
        }
        boDem = new Chung.BoDemJitter(doSau);
        ghep = new Chung.GhepKhung();
        giaiMa = new Chung.GiaiMaVideo();
        hang = new ArrayDeque<>();
        lichSu = new ArrayDeque<>();
        dongHoAm = lucPhatAm = lucNhanAm = null;
        tsAmMoi = tsHinhMoi = null;
        dongHoBao = null;
        hien = null;
        soHien = 0;
    }

    // ------------------------------------------------------------ kết nối
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
            Chung.guiJson(s.getOutputStream(), Json.obj("loai", "chao", "vai_tro", "xem", "ten", ten));
            tl = Chung.docDong(tep);
            if (tl == null) tl = Map.of();
            if ("loi".equals(tl.get("loai"))) throw new IOException("server từ chối: " + tl.get("ly_do"));
            if (!"chao".equals(tl.get("loai"))) throw new IOException("server không trả lời 'chao'");
            s.setSoTimeout(0);
            u = new DatagramSocket();
            u.setReceiveBufferSize(1 << 20);
            u.connect(new InetSocketAddress(host, (int) Chung.so(tl.get("cong_udp"), cong + 1)));   // chỉ nhận từ server
        } catch (IOException | RuntimeException e) {
            s.close();
            throw new IOException(e.getMessage(), e);
        }
        int m = (int) Chung.so(tl.get("ma"), 0);
        synchronized (khoa) {
            tcp = s;
            udp = u;
            ma = m;
            rng = new Random(m);
            lamMoiPhien();
            lamMoiThongKe();
            choPhat = !Chung.dung(tl.get("phat"), false);
            daKetNoi = true;
        }
        if (!(amThanh && video)) gui(Json.obj("loai", "dang_ky", "am_thanh", amThanh, "video", video));
        bao.accept("Vào xem: mã " + m + ", " + (Chung.dung(tl.get("phat"), false) ? "máy phát đang phát" : "chưa có ai phát"));
        // thong_tin, dong_ho đã lưu được server gửi ngay sau "chao": xử lý trước khi đăng ký UDP để khung I đầu tiên
        // (có thể tới từ bộ đệm GOP chỉ vài ms sau) đã có báo cáo đồng hồ mà ước lượng "I cũ bao nhiêu".
        try {
            double han = Chung.bayGio() + (Chung.dung(tl.get("phat"), false) ? 0.1 : 0.0);
            while (true) {
                if (tep.available() > 0) {
                    if (!xuLyTin(Chung.docDong(tep))) break;
                } else if (Chung.bayGio() < han && dongHoBaoChua()) {
                    Chung.ngu(0.002);
                } else {
                    break;
                }
            }
        } catch (IOException ignored) { }
        byte[] g = Chung.goiGiu(m);
        u.send(new DatagramPacket(g, g.length));                    // đăng ký địa chỉ UDP
        Chung.chayNen("xem-tcp", () -> docTcp(tep));
        Chung.chayNen("xem-udp", () -> nhanUdp(u));
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

    void ngat() {
        Socket s;
        DatagramSocket u;
        synchronized (khoa) {
            s = tcp;
            u = udp;
            tcp = null;
            udp = null;
            daKetNoi = false;
        }
        if (s != null) {
            try { s.shutdownInput(); s.shutdownOutput(); } catch (IOException ignored) { }
            try { s.close(); } catch (IOException ignored) { }
        }
        if (u != null) u.close();
    }

    private void docTcp(InputStream tep) {
        try {
            while (xuLyTin(Chung.docDong(tep))) { }
        } catch (IOException ignored) { }
        if (daKetNoi) {
            bao.accept("Mất kết nối server");
            ngat();
        }
    }

    private boolean dongHoBaoChua() { synchronized (khoa) { return dongHoBao == null; } }

    /** Xử lý một thông điệp TCP; false = dừng đọc (kết nối đóng hoặc thông điệp sai dạng). */
    private boolean xuLyTin(Map<String, Object> tin) {
        if (tin == null) return false;
        Object loai = tin.get("loai");
        if ("thong_tin".equals(loai)) {
            synchronized (khoa) {
                thongTin = tin;
                if (choPhat) {                                // vào trước khi phát: đo từ lúc bắt đầu phát
                    choPhat = false;
                    if (khungDauMs == null) lucMoc = Chung.bayGio();
                }
            }
            bao.accept("Luồng " + Chung.chu(tin.get("rong")) + "x" + Chung.chu(tin.get("cao")) + ", "
                    + Chung.chu(tin.get("fps")) + " fps, GOP " + Chung.chu(tin.get("gop")) + ", "
                    + Chung.chu(tin.get("tan_so_mau")) + " Hz");
        } else if ("dong_ho".equals(loai)) {
            if (!(tin.get("ts") instanceof Number) || !(tin.get("gio") instanceof Number)) return false;
            synchronized (khoa) { dongHoBao = new double[] {Chung.so(tin.get("ts"), 0), Chung.so(tin.get("gio"), 0)}; }
        } else if ("su_kien".equals(loai)) {
            bao.accept("Sự kiện: " + Chung.chu(tin.get("noi_dung")));
        } else if ("ket_thuc".equals(loai)) {
            synchronized (khoa) {
                thongTin = null;
                choPhat = true;
                lamMoiPhien();
            }
            bao.accept("Máy phát kết thúc phát");
        } else if ("bi_ngat".equals(loai)) {
            bao.accept("Bị ngắt: " + Chung.chu(tin.get("ly_do")));
        }
        return true;
    }

    // ------------------------------------------------------------ luồng nhận UDP
    /** Ước lượng ms phương tiện hiện tại (để biết khung I đầu tiên "cũ" bao nhiêu). */
    private Double uocLuongTs() {
        if (dongHoBao != null) return dongHoBao[0] + (Chung.gioUnixMs() - dongHoBao[1]);   // giả định cùng đồng hồ
        Double m = null;
        if (tsHinhMoi != null && tsHinhMoi != 0) m = (double) tsHinhMoi;
        if (tsAmMoi != null && tsAmMoi != 0) m = m == null ? tsAmMoi * Chung.MS_MOI_MAU : Math.max(m, tsAmMoi * Chung.MS_MOI_MAU);
        return m;
    }

    private void nhanUdp(DatagramSocket u) {
        byte[] buf = new byte[2048];
        DatagramPacket dp = new DatagramPacket(buf, buf.length);
        while (true) {
            int n;
            try {
                dp.setLength(buf.length);
                u.receive(dp);
                n = dp.getLength();
            } catch (IOException e) {
                return;
            }
            if (n == 0) continue;
            if (matGoi > 0 && rng.nextDouble() < matGoi) {
                boGiaLap++;                                          // "mạng" làm rơi gói ở chiều xuống
                continue;
            }
            byte[] goi = Arrays.copyOf(buf, n);
            double bayGio = Chung.bayGio();
            synchronized (khoa) {
                byteNhan += n;
                wByte += n;
                if ((goi[0] & 0xFF) == Chung.DAU_RTP) {
                    Chung.Rtp r = Chung.tachRtp(goi, n);
                    if (r != null && r.pt() == Chung.PT_AM_THANH && r.payload().length == Chung.BYTE_MOI_KHUNG && amThanh) {
                        boDem.nhanGoi(r.seq(), r.ts(), r.payload());
                        lucNhanAm = bayGio;
                        tsAmMoi = r.ts();
                        goiAm++;
                        wAm++;
                    }
                } else if (goi[0] == Chung.PHIEN_BAN && video) {
                    Chung.Header h = Chung.tachHeader(goi, n);
                    if (h == null || (h.loai() != Chung.LOAI_I && h.loai() != Chung.LOAI_P)) continue;
                    Chung.Khung k = ghep.them(h.loai(), h.so(), h.i(), h.tong(), h.w(), h.h(), h.ts(), h.payload(), bayGio);
                    if (k != null) nhanKhung(k, bayGio);
                }
            }
        }
    }

    /** Khung vừa đủ mảnh -> giải mã (có thể giải thêm các khung đang chờ) -> hàng hiển thị. */
    private void nhanKhung(Chung.Khung k, double bayGio) {
        for (Chung.KhungGiai kh : giaiMa.them(k, bayGio)) {
            tsHinhMoi = Math.max(tsHinhMoi == null ? 0 : tsHinhMoi, kh.ts());
            if (kh.loai() == Chung.LOAI_I) {
                khungI++;
                if (iDau == null) {
                    Double uoc = uocLuongTs();
                    iDau = new Double[] {uoc == null ? null : uoc - kh.ts(), (bayGio - lucMoc) * 1000};
                }
                if (dangDung) {
                    dangDung = false;
                    double ms = giaiMa.lucMat != null ? (bayGio - giaiMa.lucMat) * 1000 : 0;
                    bao.accept(Chung.f("Hồi phục bằng khung I #%d sau %.0f ms", kh.so(), ms));
                }
            } else {
                khungP++;
            }
            hang.addLast(kh);
            if (hang.size() > Chung.HANG_HIEN_THI) {
                hang.pollFirst();
                boCu++;
            }
        }
    }

    // ------------------------------------------------------------ luồng phát 20 ms (đồng hồ chủ)
    private void vongAm() {
        double keTiep = Chung.bayGio();
        long n = 0;
        short[] imLang = new short[Chung.MAU_MOI_KHUNG];
        while (song) {
            n++;
            short[] mau = null;
            DatagramSocket u;
            Integer mMa;
            synchronized (khoa) {
                u = udp;
                mMa = ma;
                Chung.BoDemJitter.Lay r = daKetNoi && amThanh ? boDem.lay() : null;
                double bayGio = Chung.bayGio();
                if (r != null) {
                    double tsMs = r.ts() * Chung.MS_MOI_MAU;
                    dongHoAm = tsMs;
                    lucPhatAm = bayGio;
                    lichSu.addLast(new double[] {tsMs, bayGio});
                    if (lichSu.size() > 150) lichSu.pollFirst();
                    if (r.payload() == null) {
                        mau = imLang;                                    // mất gói: phát im lặng
                    } else {
                        mau = Chung.byteSangPcm(r.payload());
                        if (dongHoBao != null) {                          // trễ đầu-cuối (cùng đồng hồ)
                            double tre = Chung.gioUnixMs() - (dongHoBao[1] + (tsMs - dongHoBao[0]));
                            treHien = tre;
                            tongTre += tre;
                            nTre++;
                            wTre.add(tre);
                        }
                    }
                }
            }
            mucAm = mau != null ? Chung.mucDbfs(mau) : Double.NEGATIVE_INFINITY;
            Loa l = loa;
            if (l != null) l.phat(mau != null ? mau : imLang);
            if (u != null && mMa != null && n % 50 == 0) {
                byte[] g = Chung.goiGiu(mMa);
                try { u.send(new DatagramPacket(g, g.length)); } catch (IOException ignored) { }   // giữ kết nối mỗi 1 s
            }
            keTiep += Chung.KHUNG_MS / 1000.0;
            double con = keTiep - Chung.bayGio();
            if (con < -0.1) keTiep = Chung.bayGio();                    // tụt quá xa: bắt nhịp lại
            else if (con > 0) Chung.ngu(con);
        }
    }

    // ------------------------------------------------------------ luồng hiển thị 5 ms
    private void vongHien() {
        while (song) {
            Chung.ngu(0.005);
            String xin = null;
            boolean guiPli = false;
            synchronized (khoa) {
                if (!daKetNoi) continue;
                double bayGio = Chung.bayGio();
                Chung.GiaiMaVideo gm = giaiMa;
                Long thieu = gm.thamChieuSo != null && !gm.doi.isEmpty() ? gm.thamChieuSo + 1 : null;
                boolean duocXin = bayGio - lucPli >= Chung.PLI_XEM_S;          // giới hạn 1 lần / 1 s
                if (video && gm.kiemTra(bayGio)) {
                    dungHinh++;
                    dangDung = true;
                    xin = "Thiếu khung #" + thieu + " -> đứng hình" + (duocXin ? ", xin khung khóa" : " (xin sau giới hạn 1 s)");
                    guiPli = duocXin;
                } else if (video && gm.matThamChieu && duocXin) {
                    xin = "Chưa có khung I: xin khung khóa";                  // bị giới hạn lúc trước / I bị mất
                    guiPli = true;
                }
                if (guiPli) {
                    lucPli = bayGio;
                    pliGui++;
                }
                ghep.donDep(bayGio);
                Chung.KhungGiai chon = chonKhung(bayGio);
                if (chon != null) hienKhung(chon, bayGio);
            }
            if (xin != null) {
                bao.accept(xin);
                if (guiPli) gui(Json.obj("loai", "xin_khung_khoa"));
            }
        }
    }

    /** Đồng hồ phát âm thanh hiện tại (ms phương tiện) hoặc null nếu tiếng không chạy. */
    private Double dongHoHienTai(double bayGio) {
        if (lucPhatAm == null || bayGio - lucPhatAm > 0.2) return null;
        return dongHoAm + Math.min(bayGio - lucPhatAm, 0.04) * 1000;
    }

    private Chung.KhungGiai chonKhung(double bayGio) {
        if (hang.isEmpty()) return null;
        if (!video) {
            hang.clear();
            return null;
        }
        // Tiếng "đang chạy": vừa nhận gói tiếng trong 0,5 s, hoặc mới vào xem chưa tới 0,5 s (chờ tiếng tới để khung
        // đầu cũng được đồng bộ). Không có tiếng -> hiện ngay, không chờ mãi.
        boolean tiengChay = lucNhanAm != null ? amThanh && bayGio - lucNhanAm < 0.5 : amThanh && bayGio - lucChao < 0.5;
        if (dongBo && tiengChay) {
            Double am = dongHoHienTai(bayGio);
            if (am == null) return null;                                   // chờ đồng hồ tiếng (bộ đệm jitter đang nạp)
            Chung.KhungGiai chon = null;
            while (!hang.isEmpty() && hang.peekFirst().ts() <= am) {
                if (chon != null) boCu++;                                  // khung quá cũ: bỏ
                chon = hang.pollFirst();
            }
            return chon;
        }
        Chung.KhungGiai chon = hang.pollLast();                            // không đồng bộ: hiện ngay khung mới nhất
        boCu += hang.size();
        hang.clear();
        return chon;
    }

    /** Giờ (đơn điệu) mà âm thanh có nhãn tsMs được/sẽ được phát – nội suy theo lịch sử phát. */
    private Double gioAm(double tsMs) {
        for (Iterator<double[]> it = lichSu.descendingIterator(); it.hasNext(); ) {
            double[] x = it.next();
            if (x[0] <= tsMs) return x[1] + (tsMs - x[0]) / 1000;
        }
        return null;
    }

    private void hienKhung(Chung.KhungGiai k, double bayGio) {
        hien = new Hien(k.w(), k.h(), k.anh(), k.ts(), k.so(), dongHoHienTai(bayGio));
        soHien++;
        khungHien++;
        wHien++;
        if (khungDauMs == null) {
            khungDauMs = (bayGio - lucMoc) * 1000;
            // Nguồn khung đầu: khung I đã "cũ" (chụp trước lúc vào > 1,5 chu kỳ khung) -> từ bộ đệm GOP;
            // I mới nhưng tới muộn -> phải chờ khung I; I mới tới ngay -> vừa đúng lúc có khung I.
            Double cu = iDau != null ? iDau[0] : null, den = iDau != null ? iDau[1] : null;
            double nguong = 1500 / Chung.so(thongTin == null ? null : thongTin.get("fps"), 10);
            String them = "";
            if (cu != null && cu > nguong) {
                nguonDau = "bộ đệm GOP";
                them = Chung.f(" (I cũ %.1f s)", cu / 1000);
            } else if (den != null && den > nguong) {
                nguonDau = "chờ khung I";
            } else {
                nguonDau = "khung I trực tiếp";
            }
            String moc = lucMoc > lucChao ? " (từ lúc phát)" : "";
            bao.accept(Chung.f("Khung đầu sau %.0f ms%s: %s%s", khungDauMs, moc, nguonDau, them));
        }
        if (choHinhLai != null) {
            bao.accept(Chung.f("Hình trở lại sau %.0f ms", (bayGio - choHinhLai) * 1000));
            choHinhLai = null;
        }
        boolean tiengChay = amThanh && lucPhatAm != null && bayGio - lucPhatAm < 0.5;
        Double gio = tiengChay ? gioAm(k.ts()) : null;
        if (gio != null) {
            double lech = (bayGio - gio) * 1000;
            lechHien = lech;
            GiaiDoan g = giaiDoan.get(giaiDoan.size() - 1);
            g.tong += lech;
            g.n++;
            wLech.add(lech);
        } else {
            lechHien = null;
        }
    }

    // ------------------------------------------------------------ điều khiển
    void datDongBo(boolean bat) {
        synchronized (khoa) {
            if (bat != dongBo) {
                dongBo = bat;
                giaiDoan.add(new GiaiDoan(bat));
            }
        }
        bao.accept("Đồng bộ tiếng-hình: " + (bat ? "BẬT" : "TẮT"));
    }

    void dangKy(boolean am, boolean vid) {
        synchronized (khoa) {
            if (!vid && video) {
                ghep.boDangDo();
                hang.clear();
            }
            if (vid && !video) choHinhLai = Chung.bayGio();
            if (!am && amThanh) {
                boDem.xoa();
                lichSu.clear();
                lucPhatAm = lucNhanAm = null;
            }
            if (am && !amThanh) {
                matCu += boDem.mat();
                boDem = new Chung.BoDemJitter(doSau);
            }
            amThanh = am;
            video = vid;
        }
        gui(Json.obj("loai", "dang_ky", "am_thanh", am, "video", vid));
        String t = am && vid ? "âm thanh+video" : am ? "âm thanh" : "video";
        bao.accept("Đăng ký nhận: " + t);
    }

    void datDoSau(int n) {
        synchronized (khoa) {
            doSau = Math.max(1, Math.min(25, n));
            boDem.datDoSau(doSau);
        }
    }

    Chup chup() {
        synchronized (khoa) {
            if (hien == null) return null;
            return new Chup(hien.w(), hien.h(), hien.rgb(), hien.ts(), hien.am(), lechHien, hien.so());
        }
    }

    // ------------------------------------------------------------ số liệu
    /** Gọi mỗi 1 s: số liệu của giây vừa qua (rồi xóa bộ đếm cửa sổ). */
    GiayQua thongKeGiay() {
        synchronized (khoa) {
            GiayQua r = new GiayQua(wAm, matCu + boDem.mat(), wHien, tb(wTre), tb(wLech), wByte * 8 / 1000.0);
            wAm = wHien = 0;
            wByte = 0;
            wTre = new ArrayList<>();
            wLech = new ArrayList<>();
            return r;
        }
    }

    private static Double tb(List<Double> ds) {
        if (ds.isEmpty()) return null;
        double s = 0;
        for (double x : ds) s += x;
        return s / ds.size();
    }

    TrangThai trangThai() {
        synchronized (khoa) {
            double giay = Math.max(Chung.bayGio() - lucChao, 0.001);
            TrangThai t = new TrangThai();
            t.daKetNoi = daKetNoi;
            t.ma = ma;
            t.thongTin = thongTin;
            t.goiAm = goiAm;
            t.matAm = matCu + boDem.mat();
            t.treTb = nTre > 0 ? tongTre / nTre : null;
            t.treHien = treHien;
            t.khungI = khungI;
            t.khungP = khungP;
            t.khungHien = khungHien;
            t.hong = hongCu + ghep.khungHong;
            t.boP = boPCu + giaiMa.boP;
            t.boCu = boCu;
            t.dungHinh = dungHinh;
            t.pliGui = pliGui;
            t.khungDauMs = khungDauMs;
            t.nguonDau = nguonDau;
            t.lechHien = lechHien;
            t.giaiDoan = new ArrayList<>();
            for (GiaiDoan g : giaiDoan) t.giaiDoan.add(g.sao());
            t.kbps = byteNhan * 8 / 1000.0 / giay;
            t.dongBo = dongBo;
            t.amThanh = amThanh;
            t.video = video;
            t.doSau = doSau;
            t.mucAm = mucAm;
            t.soHien = soHien;
            t.hien = hien;
            t.boGiaLap = boGiaLap;
            return t;
        }
    }

    void dong() {
        song = false;
        ngat();
        Loa l = loa;
        if (l != null) l.dong();
    }

    // ------------------------------------------------------------ in (dùng chung dòng lệnh, giao diện)
    /** Số ms để in: "–" nếu chưa có; dau = true in dấu +/−. */
    static String chuMs(Double x, boolean dau) {
        if (x == null) return "–";
        return dau ? Chung.f("%+.0f", x) : Chung.f("%.0f", x);
    }

    /** Các dòng TỔNG KẾT của người xem (dùng chung cho dòng lệnh). */
    static List<String> dongTongKet(TrangThai t, double treVao) {
        List<String> ds = new ArrayList<>(List.of(
                "===== TỔNG KẾT =====",
                (treVao > 0 ? Chung.f("vào muộn %.1f s; ", treVao) : "vào ngay; ")
                        + (t.khungDauMs != null ? Chung.f("khung đầu sau %.0f ms", t.khungDauMs) : "chưa có khung nào"),
                "  nguồn khung đầu: " + (t.nguonDau.isEmpty() ? "–" : t.nguonDau),
                "tiếng: nhận " + t.goiAm + " gói, mất " + t.matAm + ", trễ TB " + chuMs(t.treTb, false) + " ms",
                "  (trễ đầu-cuối: giả định cùng đồng hồ)",
                "hình: nhận " + (t.khungI + t.khungP) + " khung (I " + t.khungI + ", P " + t.khungP + "), hỏng " + t.hong,
                "  P bỏ chờ I " + t.boP + "; hiện " + t.khungHien + ", bỏ cũ " + t.boCu,
                "đứng hình " + t.dungHinh + ", xin khung khóa " + t.pliGui,
                "lệch tiếng-hình TB (+ = hình chậm hơn tiếng):"));
        for (int i = 0; i < t.giaiDoan.size(); i++) {
            GiaiDoan g = t.giaiDoan.get(i);
            String bt = g.dongBo ? "BẬT" : "TẮT";
            if (g.n > 0) ds.add(Chung.f("  %d. đồng bộ %s: %+.0f ms (n=%d)", i + 1, bt, g.tong / g.n, g.n));
            else ds.add(Chung.f("  %d. đồng bộ %s: – (không có khung)", i + 1, bt));
        }
        ds.add(Chung.f("nhận TB %.0f kbit/s", t.kbps));
        return ds;
    }

    record AnhRgb(int w, int h, byte[] rgb) {}

    /** Ảnh chụp có chú thích: khung đang hiện phóng ×2 + dải chữ nhãn hình, đồng hồ tiếng, lệch. */
    static AnhRgb anhChup(Chup c) {
        int w = c.w(), h = c.h(), W = 2 * w, H = 2 * h + 62;
        byte[] ra = new byte[W * H * 3];
        System.arraycopy(Chung.phongTo(c.rgb(), w, h, 2), 0, ra, 0, W * 2 * h * 3);
        Chung.toMau(ra, W, 0, 2 * h, W, 62, new int[] {20, 20, 20});
        Chung.veChu(ra, W, H, 6, 2 * h + 6, Chung.f("HINH  T=%.3f #%d", c.ts() / 1000.0, c.so()), 2, Chung.TRANG);
        Chung.veChu(ra, W, H, 6, 2 * h + 24, "TIENG " + (c.am() == null ? "-" : Chung.f("T=%.3f", c.am() / 1000)), 2, Chung.TRANG);
        String chu;
        int[] mau;
        if (c.lech() == null) {
            chu = "LECH -";
            mau = new int[] {200, 200, 200};
        } else {
            double l = c.lech();
            chu = Chung.f("LECH %+.0f MS", l) + (l < -40 ? " HINH SOM" : l > 40 ? " HINH TRE" : "");
            mau = Math.abs(l) <= 40 ? new int[] {120, 255, 120} : new int[] {255, 120, 120};
        }
        Chung.veChu(ra, W, H, 6, 2 * h + 42, chu, 2, mau);
        Chung.toMau(ra, W, 0, 2 * h, W, 2, new int[] {255, 255, 0});
        return new AnhRgb(W, H, ra);
    }
}
