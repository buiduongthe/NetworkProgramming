import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.function.Consumer;

/**
 * Một phiên client video cho bản giao diện (KhachVideoGui) – phần mạng và camera, KHÔNG chạm vào điều khiển Swing.
 * Cùng giao thức với KhachVideo (dòng lệnh):
 *  - luồng đọc TCP: tin "camera" (được gửi không, fps tối đa, độ phân giải), "nhan", "su_kien", "tu_choi";
 *  - luồng nhận UDP: ghép mảnh khung lưới của server, giải nén -> luoiMoi;
 *  - luồng gửi: mỗi 2 ms kiểm tra hạn; lấy khung camera mới nhất -> thu nhỏ 1/chia -> nén zlib -> chia mảnh -> gửi.
 * Giao diện đọc các trường volatile bằng bộ hẹn giờ Swing. Quyền riêng tư: vừa vào phòng client gửi ngay
 * "tu_tat_camera" – camera chỉ mở khi người dùng bấm START.
 */
final class PhienVideo {
    record Anh(byte[] rgb, int w, int h) {}

    final String ten;
    final Consumer<String> nhatKy;
    final Runnable khiNgat;
    Socket tcp;
    DatagramSocket udp;
    int ma;
    final Video.GhepKhung ghep = new Video.GhepKhung();
    static final int FPS_CAMERA = 15;                               // tốc độ thu; gửi thực tế = min(15, fps server cho phép)

    volatile boolean dangChay;
    volatile boolean camMoServer;                                   // tin "camera" gần nhất
    volatile String lyDo = "—";
    volatile int fpsServer = Video.FPS_MAC_DINH, chia = 1;           // giới hạn gửi do server đặt
    volatile int nhanFps = Video.FPS_MAC_DINH, nhanChia = 1;         // tham số nhận server xác nhận
    volatile boolean tuTat = true;
    volatile CameraFfmpeg.NguonCamera nguon;                         // null: chưa START
    volatile Anh khungMinh, luoiMoi;                                 // xem trước camera của mình / lưới nhận
    volatile double fpsGui, kbpsGui, fpsNhan, kbpsNhan;              // đo theo cửa sổ 1 giây
    private int demGui, byteGui, demNhan, byteNhan;                  // demNhan, byteNhan: giữ khóa ghep

    PhienVideo(String ten, Consumer<String> nhatKy, Runnable khiNgat) {
        this.ten = ten;
        this.nhatKy = nhatKy;
        this.khiNgat = khiNgat;
    }

    /** Kết nối và vào phòng (luồng nền). */
    void ketNoi(String host, int cong) throws IOException {
        tcp = new Socket();
        tcp.connect(new InetSocketAddress(host, cong), 3000);
        tcp.setTcpNoDelay(true);
        tcp.setSoTimeout(3000);
        BufferedReader sc = new BufferedReader(new InputStreamReader(tcp.getInputStream(), StandardCharsets.UTF_8));
        Video.guiJSON(tcp, Json.obj("loai", "vao", "ten", ten));
        Map<String, Object> chao = Video.docJSON(String.valueOf(sc.readLine()));
        if (chao == null || !(chao.get("ma") instanceof Number)) {
            tcp.close();
            throw new IOException("server không chào");
        }
        tcp.setSoTimeout(0);
        ma = Video.soNguyen(chao.get("ma"), 0);
        Video.guiJSON(tcp, Json.obj("loai", "tu_tat_camera"));      // camera tắt cho tới khi bấm START
        udp = new DatagramSocket();
        udp.setReceiveBufferSize(1 << 20);
        udp.connect(new InetSocketAddress(host, Video.soNguyen(chao.get("cong_udp"), 0)));
        dangChay = true;
        nhatKy.accept("Vào phòng, mã " + ma);
        Thread.ofPlatform().daemon().name("tcp-" + ten).start(() -> docTCP(sc));
        Thread.ofPlatform().daemon().name("udp-" + ten).start(this::nhanUDP);
        Thread.ofPlatform().daemon().name("gui-" + ten).start(this::vongGui);
    }

    private void docTCP(BufferedReader sc) {
        try {
            String line;
            while ((line = sc.readLine()) != null) {
                Map<String, Object> tin = Video.docJSON(line);
                if (tin == null) break;
                Object loai = tin.get("loai");
                if ("camera".equals(loai)) {
                    camMoServer = Boolean.TRUE.equals(tin.get("mo"));
                    lyDo = String.valueOf(tin.get("ly_do"));
                    fpsServer = Video.soNguyen(tin.get("fps"), fpsServer);
                    chia = Video.soNguyen(tin.get("chia"), chia);
                    nhatKy.accept("Camera: " + lyDo + ", gửi tối đa " + fpsServer + " fps, 1/" + chia);
                } else if ("nhan".equals(loai)) {
                    nhanFps = Video.soNguyen(tin.get("fps"), nhanFps);
                    nhanChia = Video.soNguyen(tin.get("chia"), nhanChia);
                    nhatKy.accept("Nhận: " + nhanFps + " fps, độ phân giải 1/" + nhanChia);
                } else if ("su_kien".equals(loai)) {
                    nhatKy.accept("Sự kiện: " + tin.get("noi_dung"));
                } else if ("tu_choi".equals(loai)) {
                    nhatKy.accept("Bị từ chối: " + tin.get("ly_do"));
                }
            }
        } catch (IOException ignored) { }
        if (dangChay) {
            nhatKy.accept("Mất kết nối tới server");
            ngat();
            khiNgat.run();
        }
    }

    private void nhanUDP() {
        byte[] buf = new byte[2048];
        while (true) {
            DatagramPacket p = new DatagramPacket(buf, buf.length);
            try {
                udp.receive(p);
            } catch (IOException e) {
                return;
            }
            Video.Manh m = Video.tachHeader(p.getData(), p.getLength());
            if (m == null || m.h().loai() != Video.LOAI_MANH) continue;
            byte[] nen;
            synchronized (ghep) { nen = ghep.them(m.h(), m.payload(), System.nanoTime()); }
            if (nen == null) continue;
            byte[] anh = Video.giaiNen(nen);
            if (anh == null || anh.length != m.h().w() * m.h().h() * 3) continue;
            luoiMoi = new Anh(anh, m.h().w(), m.h().h());
            synchronized (ghep) {
                demNhan++;
                byteNhan += nen.length;
            }
        }
    }

    /** Vòng gửi: xem trước theo 15 fps; gửi theo min(15, fps server) khi cả hai phía cho phép (quy tắc 1). */
    private void vongGui() {
        long bayGio = System.nanoTime(), hanXem = bayGio, hanGui = bayGio, mocDo = bayGio, hanGiu = bayGio + 1_000_000_000L;
        int soXem = 0, soKhung = 0;
        byte[] giu = Video.goiGiu(ma);
        try {
            udp.send(new DatagramPacket(giu, giu.length));          // đăng ký địa chỉ UDP với server
            while (dangChay) {
                bayGio = System.nanoTime();
                CameraFfmpeg.NguonCamera n = nguon;
                if (n != null && n.loi() != null && !n.loi().equals("đã đóng")) {   // ffmpeg dừng (camera bận…)
                    nhatKy.accept(n.loi() + " – chuyển sang camera giả lập");
                    nguon = n = new CameraFfmpeg.GiaLap(ten, ma);
                }
                if (n != null && bayGio >= hanXem) {
                    hanXem = Math.max(hanXem + 1_000_000_000L / FPS_CAMERA, bayGio - 100_000_000L);
                    byte[] k = n.layKhung(soXem++);
                    if (k != null) khungMinh = new Anh(k, Video.RONG, Video.CAO);
                }
                Anh k = khungMinh;
                if (n != null && k != null && !tuTat && camMoServer && bayGio >= hanGui) {
                    hanGui += 1_000_000_000L / Math.min(FPS_CAMERA, fpsServer);
                    if (hanGui < bayGio) hanGui = bayGio;
                    Video.Anh a = Video.thuNho(k.rgb(), k.w(), k.h(), chia);   // giảm độ phân giải rồi nén
                    byte[] nen = Video.nenZlib(a.rgb());
                    for (byte[] g : Video.chiaManh(ma, soKhung, a.w(), a.h(), nen)) udp.send(new DatagramPacket(g, g.length));
                    soKhung++;
                    demGui++;
                    byteGui += nen.length;
                } else if ((tuTat || !camMoServer) && bayGio >= hanGiu) {
                    udp.send(new DatagramPacket(giu, giu.length));    // giữ kết nối khi không gửi
                    hanGiu = bayGio + 1_000_000_000L;
                }
                if (bayGio - mocDo >= 1_000_000_000L) {
                    mocDo += 1_000_000_000L;
                    fpsGui = demGui;
                    kbpsGui = byteGui * 8 / 1000.0;
                    demGui = byteGui = 0;
                    synchronized (ghep) {
                        fpsNhan = demNhan;
                        kbpsNhan = byteNhan * 8 / 1000.0;
                        demNhan = byteNhan = 0;
                    }
                }
                Thread.sleep(2);
            }
        } catch (IOException | InterruptedException e) {
            if (dangChay) nhatKy.accept("lỗi vòng gửi: " + e.getMessage());
        }
    }

    /** START: dùng nguồn n, xin server bật camera (phải chờ server xác nhận). */
    void batCamera(CameraFfmpeg.NguonCamera n) {
        CameraFfmpeg.NguonCamera cu = nguon;
        nguon = n;
        if (cu != null && cu != n) cu.dong();
        tuTat = false;
        Video.guiJSON(tcp, Json.obj("loai", "tu_bat_camera"));
    }

    /** CANCEL: tự tắt có hiệu lực ngay; đóng camera (dừng ffmpeg). */
    void tatCamera() {
        tuTat = true;
        Video.guiJSON(tcp, Json.obj("loai", "tu_tat_camera"));
        CameraFfmpeg.NguonCamera cu = nguon;
        nguon = null;
        khungMinh = null;
        if (cu != null) cu.dong();
    }

    /** Tham số nhận (quy tắc 5): fps và độ phân giải lưới 1/1 hoặc 1/2. */
    void chonNhan(int fps, int chiaNhan) {
        if (dangChay) Video.guiJSON(tcp, Json.obj("loai", "nhan", "fps", fps, "chia", chiaNhan));
    }

    void ngat() {
        dangChay = false;
        try { if (tcp != null) tcp.close(); } catch (IOException ignored) { }
        if (udp != null) udp.close();
        CameraFfmpeg.NguonCamera n = nguon;
        nguon = null;
        if (n != null) n.dong();
    }
}
