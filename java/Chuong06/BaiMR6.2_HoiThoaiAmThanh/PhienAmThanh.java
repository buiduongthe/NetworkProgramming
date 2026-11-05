import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Map;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;

/**
 * Một phiên client âm thanh cho bản giao diện (KhachAmThanhGui) – phần mạng và âm thanh, KHÔNG có điều khiển
 * Swing nào. Cùng giao thức với KhachAmThanh (dòng lệnh):
 *  - luồng đọc TCP: tin "mic", "do_loi", "su_kien", "tu_choi" -> ghi vào các trường volatile và báo nhật ký;
 *  - luồng nhận UDP: gói RTP của luồng trộn -> bộ đệm jitter;
 *  - nhịp 20 ms (ScheduledExecutorService): lấy khung micro -> gửi RTP nếu được phép; lấy khung từ bộ đệm jitter
 *    -> nhân âm lượng nghe -> đo mức -> phát loa (nếu Start Audio).
 * Giao diện đọc các trường volatile bằng bộ hẹn giờ Swing; các hàm gọi lại (nhatKy, khiNgat) chạy trên luồng
 * nền, nên phía giao diện phải chuyển sang luồng giao diện bằng SwingUtilities.invokeLater.
 * Quyền riêng tư: vừa vào phòng client gửi ngay "tu_tat_mic" – mic chỉ mở khi người dùng bấm Start Mic.
 */
final class PhienAmThanh {
    final String ten;
    final Consumer<String> nhatKy;
    final Runnable khiNgat;
    Socket tcp;
    DatagramSocket udp;
    int ma;
    ScheduledExecutorService nhip;
    final Object khoa = new Object();
    final AmThanh.BoDemJitter boDem = new AmThanh.BoDemJitter(3);

    // ---- Trạng thái chia sẻ với giao diện (đọc/ghi từ nhiều luồng -> volatile)
    volatile boolean dangChay;
    volatile boolean micMoServer;              // tin "mic" gần nhất của server
    volatile String lyDoMic = "—";
    volatile double doLoiServer = Double.NaN;   // tin "do_loi" (NaN: server cũ không gửi)
    volatile boolean tuTat = true;              // người dùng chưa bấm Start Mic / đã bấm Stop Mic
    volatile double amLuongNghe = 1, amLuongMic = 1;
    volatile ThietBiAmThanh.NguonKhung nguon;   // null: chưa Start Mic
    volatile ThietBiAmThanh.LoaPhat loa;        // null: không phát (Stop Audio hoặc không có loa)
    volatile double mucNhan = Double.NEGATIVE_INFINITY, mucMic = Double.NEGATIVE_INFINITY;
    volatile int goiGui, goiNhan;

    private int seq, demGiu;
    private long ts;
    private final short[] ganDay = new short[AmThanh.TAN_SO_MAU];   // 1 giây âm thanh nhận gần nhất
    private int viTri;

    PhienAmThanh(String ten, Consumer<String> nhatKy, Runnable khiNgat) {
        this.ten = ten;
        this.nhatKy = nhatKy;
        this.khiNgat = khiNgat;
    }

    /** Kết nối và vào phòng (chạy trên luồng nền – có thể chờ tới 3 s). Ném IOException nếu không vào được. */
    void ketNoi(String host, int cong) throws IOException {
        tcp = new Socket();
        tcp.connect(new InetSocketAddress(host, cong), 3000);
        tcp.setTcpNoDelay(true);
        tcp.setSoTimeout(3000);                                   // chờ "chao" tối đa 3 s
        BufferedReader sc = new BufferedReader(new InputStreamReader(tcp.getInputStream(), StandardCharsets.UTF_8));
        AmThanh.guiJSON(tcp, Json.obj("loai", "vao", "ten", ten));
        Map<String, Object> chao = ServerAmThanh.docJSON(String.valueOf(sc.readLine()));
        if (chao == null || !(chao.get("ma") instanceof Number m) || !(chao.get("cong_udp") instanceof Number cu)) {
            tcp.close();
            throw new IOException("server không chào");
        }
        tcp.setSoTimeout(0);
        ma = m.intValue();
        AmThanh.guiJSON(tcp, Json.obj("loai", "tu_tat_mic"));     // mic tắt cho tới khi bấm Start Mic
        udp = new DatagramSocket();
        udp.connect(new InetSocketAddress(host, cu.intValue()));  // chỉ nhận gói từ server
        dangChay = true;
        nhatKy.accept("Vào phòng, mã " + ma + "; UDP server cổng " + cu.intValue());
        Thread.ofPlatform().daemon().name("tcp-" + ten).start(() -> docTCP(sc));
        Thread.ofPlatform().daemon().name("udp-" + ten).start(this::nhanUDP);
        byte[] giu = AmThanh.dongGoiRTP(AmThanh.PT_GIU_KET_NOI, 0, 0, ma, new byte[0]);
        udp.send(new DatagramPacket(giu, giu.length));            // đăng ký địa chỉ UDP với server ngay
        nhip = Executors.newSingleThreadScheduledExecutor(r -> Thread.ofPlatform().daemon().name("nhip-" + ten).unstarted(r));
        nhip.scheduleAtFixedRate(() -> {
            try {
                vong20ms();
            } catch (RuntimeException | IOException e) {
                if (dangChay) nhatKy.accept("lỗi nhịp 20 ms: " + e);
            }
        }, AmThanh.KHUNG_MS, AmThanh.KHUNG_MS, TimeUnit.MILLISECONDS);
    }

    private void docTCP(BufferedReader sc) {
        try {
            String line;
            while ((line = sc.readLine()) != null) {
                Map<String, Object> tin = ServerAmThanh.docJSON(line);
                if (tin == null) break;
                Object loai = tin.get("loai");
                if ("mic".equals(loai)) {
                    micMoServer = Boolean.TRUE.equals(tin.get("mo"));
                    lyDoMic = String.valueOf(tin.get("ly_do"));
                    nhatKy.accept("Mic: " + lyDoMic);
                } else if ("do_loi".equals(loai)) {                    // tin mới: độ lợi server đặt cho mình
                    if (tin.get("gia_tri") instanceof Number g) doLoiServer = g.doubleValue();
                } else if ("su_kien".equals(loai)) {
                    nhatKy.accept("Sự kiện: " + tin.get("noi_dung"));
                } else if ("tu_choi".equals(loai)) {
                    nhatKy.accept("Bị từ chối: " + tin.get("ly_do"));
                }
            }
        } catch (IOException ignored) { }
        if (dangChay) {                                           // server đóng kết nối (không phải mình ngắt)
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
                return;                                           // socket đã đóng
            }
            AmThanh.GoiRTP g = AmThanh.tachRTP(p.getData(), p.getLength());
            if (g != null && g.pt() == AmThanh.PT_AM_THANH && g.payload().length == AmThanh.BYTE_MOI_KHUNG) {
                synchronized (khoa) { boDem.nhanGoi(g.seq(), g.ts(), g.payload(), AmThanh.giay()); }
                goiNhan++;
            }
        }
    }

    /** Mỗi 20 ms: thu – gửi, rồi nhận – phát. */
    private void vong20ms() throws IOException {
        // 1. Thu và gửi
        ThietBiAmThanh.NguonKhung n = nguon;
        short[] khung = n != null ? n.layKhung() : null;
        if (khung != null) {
            khung = ThietBiAmThanh.nhan(khung, amLuongMic);       // Mic Volume
            mucMic = AmThanh.mucDBFS(khung, khung.length);
        } else mucMic = Double.NEGATIVE_INFINITY;
        if (khung != null && !tuTat && micMoServer) {             // quy tắc 1: chỉ gửi khi cả hai phía cho phép
            byte[] goi = AmThanh.dongGoiRTP(AmThanh.PT_AM_THANH, seq, ts, ma, AmThanh.pcmSangByte(khung));
            udp.send(new DatagramPacket(goi, goi.length));
            seq = (seq + 1) & 0xFFFF;
            goiGui++;
        } else if (++demGiu % 50 == 0) {                          // mic tắt: mỗi giây một gói giữ kết nối
            byte[] giu = AmThanh.dongGoiRTP(AmThanh.PT_GIU_KET_NOI, 0, 0, ma, new byte[0]);
            udp.send(new DatagramPacket(giu, giu.length));
        }
        ts = (ts + AmThanh.MAU_MOI_KHUNG) & 0xFFFFFFFFL;
        // 2. Nhận và phát
        byte[] p;
        synchronized (khoa) { p = boDem.lay(); }
        short[] mau = p != null ? AmThanh.byteSangPCM(p) : new short[AmThanh.MAU_MOI_KHUNG];
        if (amLuongNghe != 1) mau = ThietBiAmThanh.nhan(mau, amLuongNghe);   // âm lượng nghe: chỉ tại client
        mucNhan = AmThanh.mucDBFS(mau, mau.length);
        synchronized (khoa) {
            for (short x : mau) {
                ganDay[viTri] = x;
                viTri = (viTri + 1) % ganDay.length;
            }
        }
        ThietBiAmThanh.LoaPhat l = loa;
        if (l != null) l.phat(mau);
    }

    /** Bản sao 1 giây âm thanh nhận gần nhất (theo thứ tự thời gian) – để phân tích tần số. */
    short[] layGanDay() {
        synchronized (khoa) {
            short[] ra = new short[ganDay.length];
            for (int i = 0; i < ra.length; i++) ra[i] = ganDay[(viTri + i) % ganDay.length];
            return ra;
        }
    }

    /** Start Mic: dùng nguồn n, xin server bật mic (quy tắc 6: phải chờ server xác nhận). */
    void batMic(ThietBiAmThanh.NguonKhung n) {
        ThietBiAmThanh.NguonKhung cu = nguon;
        nguon = n;
        if (cu != null && cu != n) cu.dong();
        tuTat = false;
        AmThanh.guiJSON(tcp, Json.obj("loai", "tu_bat_mic"));
    }

    /** Stop Mic: tự tắt có hiệu lực ngay, không cần server đồng ý; đóng thiết bị thu. */
    void tatMic() {
        tuTat = true;
        AmThanh.guiJSON(tcp, Json.obj("loai", "tu_tat_mic"));
        ThietBiAmThanh.NguonKhung cu = nguon;
        nguon = null;
        if (cu != null) cu.dong();
    }

    /** Âm lượng nghe: áp dụng tại client và báo cho server để hiển thị (quy tắc 5). */
    void datAmLuongNghe(double v) {
        amLuongNghe = v;
        if (dangChay) AmThanh.guiJSON(tcp, Json.obj("loai", "am_luong_nghe", "gia_tri", v));
    }

    void datLoa(ThietBiAmThanh.LoaPhat l) {
        ThietBiAmThanh.LoaPhat cu = loa;
        loa = l;
        if (cu != null && cu != l) cu.dong();
    }

    /** Disconnect: dừng nhịp, đóng socket và thiết bị. Gọi được nhiều lần. */
    void ngat() {
        dangChay = false;
        if (nhip != null) nhip.shutdownNow();
        try { if (tcp != null) tcp.close(); } catch (IOException ignored) { }
        if (udp != null) udp.close();
        ThietBiAmThanh.NguonKhung n = nguon;
        nguon = null;
        if (n != null) n.dong();
        datLoa(null);
        micMoServer = false;
        mucNhan = mucMic = Double.NEGATIVE_INFINITY;
    }
}
