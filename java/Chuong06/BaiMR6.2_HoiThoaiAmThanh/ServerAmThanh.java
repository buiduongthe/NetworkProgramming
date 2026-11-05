import java.io.BufferedReader;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketAddress;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;
import java.util.concurrent.Executors;
import java.util.concurrent.ScheduledExecutorService;
import java.util.concurrent.TimeUnit;
import java.util.function.Consumer;
import java.util.function.Supplier;

/**
 * Bài mở rộng MR6.2 – Server phòng hội thoại âm thanh: điều khiển JSON trên TCP, âm thanh RTP trên UDP,
 * trộn tiếng "mix-minus" mỗi 20 ms, bảng điều khiển đọc stdin.
 *
 * Quy tắc quyền điều khiển (giống bản Python, Go):
 *  1. Mic của client X mở khi và chỉ khi X không tự tắt VÀ server không khóa mic của X.
 *  2. Server tắt (khóa) thắng client: khi đang bị khóa, client xin bật mic bị từ chối.
 *  3. Server bật = gỡ khóa, KHÔNG bật hộ mic mà người dùng đã tự tắt (quyền riêng tư).
 *  4. Độ lợi đầu vào từng client và âm lượng chung do server đặt, áp dụng khi trộn.
 *  5. Âm lượng nghe do từng client tự đặt và áp dụng tại client; server không đổi được.
 *  6. Server tự thực thi, không tin client: bỏ gói từ client đang tắt mic, bỏ gói sai địa chỉ nguồn.
 *
 * Chạy: java -cp out ServerAmThanh [host] [cổng_tcp] [cổng_udp]
 * Lệnh (stdin): tat_mic <tên|tat_ca>, bat_mic <tên|tat_ca>, do_loi <tên> <hệ số>, am_luong_chung <hệ số>,
 *               trang_thai, thoat. Dòng "@t lệnh" = chờ tới giây thứ t kể từ khi chương trình chạy.
 *
 * Mỗi đối tượng ServerAmThanh là một phòng (một server): bản dòng lệnh (main bên dưới) tạo một đối tượng và
 * đọc lệnh từ stdin; bản giao diện ServerAmThanhGui tạo đối tượng mới mỗi lần bấm Start Server và gọi thẳng các
 * hàm tatBatMic, datDoLoi, datAmLuongChung. Hai điểm móc micServer, loaServer (null ở bản dòng lệnh) cho server
 * tự nói và tự nghe. So với bản đầu, giao thức chỉ THÊM tin {"loai":"do_loi","gia_tri":g} gửi cho client mỗi
 * khi độ lợi của client đó đổi (và một lần sau "chao") – client cũ bỏ qua loại tin không biết.
 */
public class ServerAmThanh {
    static final class Khach {
        final int ma;
        final String ten, ip;
        final Socket conn;
        SocketAddress udp;                         // địa chỉ UDP học được từ gói đầu tiên
        boolean tuTat;                             // client tự tắt mic
        boolean serverKhoa;                        // server khóa mic
        double doLoi = 1;                          // độ lợi đầu vào (gain)
        double amLuongNghe = 1;                    // client báo (chỉ để hiển thị)
        final AmThanh.BoDemJitter boDem = new AmThanh.BoDemJitter(3);
        int seqRa;                                 // số thứ tự, nhãn thời gian của luồng trộn gửi cho client
        long tsRa;
        double mucVao = Double.NEGATIVE_INFINITY;
        int biChan;                                // gói bị bỏ vì mic đang tắt
        int saiNguon;                              // gói bị bỏ vì sai địa chỉ nguồn
        int catNgon;                               // số mẫu bị cắt ngọn trong luồng gửi cho client này

        Khach(int ma, String ten, String ip, Socket conn) { this.ma = ma; this.ten = ten; this.ip = ip; this.conn = conn; }

        boolean micMo() { return !tuTat && !serverKhoa; }   // quy tắc 1

        String lyDo() {
            if (micMo()) return "mở";
            if (tuTat && serverKhoa) return "tắt (cả hai)";
            return serverKhoa ? "tắt (server)" : "tắt (tự tắt)";
        }

        void gui(Map<String, Object> v) { AmThanh.guiJSON(conn, v); }

        void guiMic() {
            gui(Json.obj("loai", "mic", "mo", micMo(), "ly_do", lyDo()));
            if (!micMo()) boDem.xoa();             // bỏ phần âm thanh còn trong bộ đệm
        }
    }

    final Object khoa = new Object();              // bảo vệ mọi trạng thái dùng chung giữa các luồng
    final TreeMap<Integer, Khach> dsKhach = new TreeMap<>();   // theo thứ tự mã
    final List<Khach> daRoi = new ArrayList<>();
    int maMoi = 0;
    double amLuongChung = 1.0;
    int congUDP;

    // ---- Phần thêm cho bản giao diện; bản dòng lệnh để null nên hành vi, đầu ra giữ nguyên
    /** Micro của máy server: mỗi 20 ms trả một khung (đã nhân Mic Volume) hoặc null. Tiếng server là một nguồn
     *  trong bộ trộn với mã SSRC_SERVER: mọi client đều nghe, không đi qua mạng tới chính server. */
    volatile Supplier<short[]> micServer;
    /** Loa của máy server: nhận tổng tiếng các client (không gồm tiếng server) mỗi 20 ms. Phải trả về ngay. */
    volatile Consumer<short[]> loaServer;
    volatile double mucMicServer = Double.NEGATIVE_INFINITY, mucLoaServer = Double.NEGATIVE_INFINITY;   // dBFS

    final Consumer<String> nhatKy;                 // nơi in nhật ký: stdout (dòng lệnh) hoặc ô nhật ký (giao diện)
    ServerSocket ln;
    DatagramSocket udp;
    ScheduledExecutorService nhip;

    ServerAmThanh(Consumer<String> nhatKy) { this.nhatKy = nhatKy; }

    void slog(String s) { nhatKy.accept(s); }

    /** Báo sự kiện cho mọi client (họ hiển thị và đánh mốc trong bản ghi). Gọi khi đang giữ khóa. */
    void phatSuKien(String text) {
        slog("  => " + text);
        for (Khach k : dsKhach.values()) k.gui(Json.obj("loai", "su_kien", "noi_dung", text));
    }

    // ---------------------------------------------------------------- Kênh điều khiển TCP
    @SuppressWarnings("unchecked")
    static Map<String, Object> docJSON(String line) {
        try {
            Object o = Json.parse(line);
            return o instanceof Map<?, ?> ? (Map<String, Object>) o : null;
        } catch (RuntimeException e) {
            return null;
        }
    }

    void phucVu(Socket c) {
        try (c) {
            BufferedReader r = new BufferedReader(new InputStreamReader(c.getInputStream(), StandardCharsets.UTF_8));
            Map<String, Object> tin = docJSON(String.valueOf(r.readLine()));
            if (tin == null || !"vao".equals(tin.get("loai")) || !(tin.get("ten") instanceof String ten) || ten.isEmpty()) return;
            if (ten.length() > 8) ten = ten.substring(0, 8);
            Khach k;
            synchronized (khoa) {
                k = new Khach(++maMoi, ten, c.getInetAddress().getHostAddress(), c);
                dsKhach.put(k.ma, k);
            }
            k.gui(Json.obj("loai", "chao", "ma", k.ma, "cong_udp", congUDP, "tan_so_mau", AmThanh.TAN_SO_MAU, "khung_ms", AmThanh.KHUNG_MS));
            k.gui(Json.obj("loai", "do_loi", "gia_tri", k.doLoi));     // độ lợi ban đầu (client hiện "Server Vol")
            synchronized (khoa) {
                slog("#" + k.ma + " " + k.ten + " vào phòng từ " + k.ip + ":" + c.getPort());
                k.guiMic();
                phatSuKien(k.ten + " vào phòng");
            }
            String line;
            while ((line = r.readLine()) != null) {
                Map<String, Object> t = docJSON(line);
                if (t == null) break;
                synchronized (khoa) { xuLyTinClient(k, t); }
            }
            synchronized (khoa) {
                dsKhach.remove(k.ma);
                daRoi.add(k);
                slog("#" + k.ma + " " + k.ten + " rời phòng");
                phatSuKien(k.ten + " rời phòng");
            }
        } catch (IOException ignored) { }
    }

    void xuLyTinClient(Khach k, Map<String, Object> tin) {
        Object loai = tin.get("loai");
        if ("tu_tat_mic".equals(loai)) {
            k.tuTat = true;
            k.guiMic();
            phatSuKien(k.ten + " tự tắt mic");
        } else if ("tu_bat_mic".equals(loai)) {
            k.tuTat = false;
            if (k.serverKhoa) {                                       // quy tắc 2
                slog(k.ten + " xin bật mic -> từ chối (server đang khóa)");
                k.gui(Json.obj("loai", "tu_choi", "ly_do", "server đang khóa mic của bạn"));
            } else {
                k.guiMic();
                phatSuKien(k.ten + " tự bật mic");
            }
        } else if ("am_luong_nghe".equals(loai)) {                    // quy tắc 5: chỉ ghi nhận
            if (tin.get("gia_tri") instanceof Number v) k.amLuongNghe = v.doubleValue();
            slog(String.format(Locale.ROOT, "%s tự đặt âm lượng nghe %.2f", k.ten, k.amLuongNghe));
        }
    }

    // ---------------------------------------------------------------- Kênh dữ liệu UDP
    void nhanUDP(DatagramSocket sock) {
        byte[] buf = new byte[2048];
        while (true) {
            DatagramPacket p = new DatagramPacket(buf, buf.length);
            try {
                sock.receive(p);
            } catch (IOException e) {
                return;
            }
            AmThanh.GoiRTP g = AmThanh.tachRTP(p.getData(), p.getLength());
            if (g == null) continue;
            SocketAddress addr = p.getSocketAddress();
            synchronized (khoa) {
                Khach k = dsKhach.get((int) g.ssrc());
                if (k == null) continue;
                if (k.udp == null && !p.getAddress().getHostAddress().equals(k.ip)) continue;
                if (k.udp == null) {                                  // đăng ký địa chỉ UDP lần đầu
                    k.udp = addr;
                    slog("#" + k.ma + " " + k.ten + " đăng ký UDP " + p.getAddress().getHostAddress() + ":" + p.getPort());
                }
                if (!addr.equals(k.udp)) k.saiNguon++;                // quy tắc 6: sai nguồn
                else if (g.pt() == AmThanh.PT_AM_THANH && g.payload().length == AmThanh.BYTE_MOI_KHUNG) {
                    if (!k.micMo()) k.biChan++;                       // quy tắc 6: mic tắt
                    else k.boDem.nhanGoi(g.seq(), g.ts(), g.payload(), AmThanh.giay());
                }
            }
        }
    }

    /**
     * Mỗi 20 ms: lấy một khung từ mỗi client đang mở mic, nhân độ lợi, cộng vào tổng T; người nghe k nhận
     * T - (tiếng của chính k) – "mix-minus" – nhân âm lượng chung, cắt ngọn, gửi đi.
     * Bản giao diện: tiếng micro server (nếu có) cũng cộng vào T; loa server phát T - tiếng server.
     */
    void vongTron(DatagramSocket sock) {
        synchronized (khoa) {
            Map<Integer, double[]> vao = new HashMap<>();
            double[] tong = new double[AmThanh.MAU_MOI_KHUNG];
            for (Khach k : dsKhach.values()) {
                byte[] p = k.micMo() ? k.boDem.lay() : null;
                if (p == null) {
                    k.mucVao = Double.NEGATIVE_INFINITY;
                    continue;
                }
                short[] mau = AmThanh.byteSangPCM(p);
                k.mucVao = AmThanh.mucDBFS(mau, mau.length);
                double[] v = new double[AmThanh.MAU_MOI_KHUNG];
                for (int i = 0; i < mau.length; i++) {
                    v[i] = k.doLoi * mau[i];
                    tong[i] += v[i];
                }
                vao.put(k.ma, v);
            }
            Supplier<short[]> mic = micServer;
            short[] tiengServer = mic != null ? mic.get() : null;      // một nguồn nữa trong bộ trộn (SSRC_SERVER)
            mucMicServer = tiengServer != null ? AmThanh.mucDBFS(tiengServer, tiengServer.length) : Double.NEGATIVE_INFINITY;
            if (tiengServer != null) for (int i = 0; i < tong.length; i++) tong[i] += tiengServer[i];
            boolean[] cat = new boolean[1];
            for (Khach k : dsKhach.values()) {
                if (k.udp == null) continue;
                double[] rieng = vao.get(k.ma);
                short[] ra = new short[AmThanh.MAU_MOI_KHUNG];
                for (int i = 0; i < ra.length; i++) {
                    double v = tong[i] - (rieng != null ? rieng[i] : 0);
                    ra[i] = AmThanh.catNgon(v * amLuongChung, cat);
                    if (cat[0]) k.catNgon++;
                }
                byte[] goi = AmThanh.dongGoiRTP(AmThanh.PT_AM_THANH, k.seqRa, k.tsRa, AmThanh.SSRC_SERVER, AmThanh.pcmSangByte(ra));
                try {
                    sock.send(new DatagramPacket(goi, goi.length, k.udp));
                } catch (IOException ignored) { }
                k.seqRa = (k.seqRa + 1) & 0xFFFF;
                k.tsRa = (k.tsRa + AmThanh.MAU_MOI_KHUNG) & 0xFFFFFFFFL;
            }
            Consumer<short[]> loa = loaServer;
            if (loa != null) {                                        // server nghe: T trừ tiếng của chính server
                short[] ra = new short[AmThanh.MAU_MOI_KHUNG];
                for (int i = 0; i < ra.length; i++)
                    ra[i] = AmThanh.catNgon((tong[i] - (tiengServer != null ? tiengServer[i] : 0)) * amLuongChung, null);
                mucLoaServer = AmThanh.mucDBFS(ra, ra.length);
                loa.accept(ra);
            } else mucLoaServer = Double.NEGATIVE_INFINITY;
        }
    }

    // ---------------------------------------------------------------- Lệnh điều khiển (dùng chung dòng lệnh – giao diện)
    List<Khach> chon(String ten) {
        List<Khach> kq = new ArrayList<>();
        for (Khach k : dsKhach.values()) if (ten.equals("tat_ca") || k.ten.equals(ten)) kq.add(k);
        return kq;
    }

    /** Server tắt (khóa) hoặc bật (gỡ khóa) mic của các client ds; ai = tên hiển thị trong sự kiện. Gọi khi giữ khóa. */
    void tatBatMic(List<Khach> ds, boolean tat, String ai) {
        List<String> vanTat = new ArrayList<>();
        for (Khach k : ds) {
            k.serverKhoa = tat;
            k.guiMic();
            if (k.tuTat) vanTat.add(k.ten);
        }
        if (tat) phatSuKien("server tắt mic " + ai);
        else phatSuKien("server bật mic " + ai + (vanTat.isEmpty() ? "" : " (" + String.join(", ", vanTat) + " vẫn tự tắt)"));   // quy tắc 3
    }

    /** Đặt độ lợi đầu vào (quy tắc 4) và báo riêng cho từng client bằng tin "do_loi". Gọi khi giữ khóa. */
    void datDoLoi(List<Khach> ds, String ai, double g) {
        for (Khach k : ds) {
            k.doLoi = g;
            k.gui(Json.obj("loai", "do_loi", "gia_tri", g));
        }
        phatSuKien(String.format(Locale.ROOT, "server đặt độ lợi %s = %.2f", ai, g));
    }

    void datAmLuongChung(double g) {
        amLuongChung = g;
        phatSuKien(String.format(Locale.ROOT, "server đặt âm lượng chung %.2f", amLuongChung));
    }

    // ---------------------------------------------------------------- Bảng điều khiển (stdin)
    boolean lenhServer(String dong) {
        String[] p = dong.strip().split("\\s+");
        if (p[0].isEmpty()) return true;
        slog(String.format(Locale.ROOT, "t=%.1fs > %s", AmThanh.giay(), dong));
        synchronized (khoa) {
            if ((p[0].equals("tat_mic") || p[0].equals("bat_mic")) && p.length == 2) {
                List<Khach> ds = chon(p[1]);
                if (ds.isEmpty()) {
                    slog("không có client " + p[1]);
                    return true;
                }
                tatBatMic(ds, p[0].equals("tat_mic"), p[1].equals("tat_ca") ? "tất cả" : p[1]);
            } else if (p[0].equals("do_loi") && p.length == 3) {
                datDoLoi(chon(p[1]), p[1], Double.parseDouble(p[2]));
            } else if (p[0].equals("am_luong_chung") && p.length == 2) {
                datAmLuongChung(Double.parseDouble(p[1]));
            } else if (p[0].equals("trang_thai")) {
                inTrangThai();
            } else if (p[0].equals("thoat")) {
                return false;
            } else {
                slog("lệnh không hợp lệ");
            }
        }
        return true;
    }

    static String padPhai(String s, int n) { return s + " ".repeat(Math.max(0, n - s.length())); }

    void inTrangThai() {
        slog(String.format(Locale.ROOT, "Trạng thái: %d client, âm lượng chung %.2f", dsKhach.size(), amLuongChung));
        slog(" #  tên mic          lợi   mức vào            mất  jit.");
        for (Khach k : dsKhach.values())
            slog(String.format(Locale.ROOT, "%2d  %-3s %s %4.2f %s %s %3d %4.1fms", k.ma, k.ten, padPhai(k.lyDo(), 12), k.doLoi,
                    AmThanh.chuDB(k.mucVao, 5), AmThanh.thanhMuc(k.mucVao, 10), k.boDem.mat(), k.boDem.jitterMs()));
    }

    void inTongKet() {
        slog("Tổng kết (gói vào: từ client tới server):");
        slog(" #  tên gói vào mất trễ thiếu cạn bị chặn cắt ngọn");
        List<Khach> ds = new ArrayList<>(dsKhach.values());
        ds.addAll(daRoi);
        ds.sort((a, b) -> Integer.compare(a.ma, b.ma));
        for (Khach k : ds) {
            AmThanh.BoDemJitter b = k.boDem;
            slog(String.format("%2d  %-3s %7d %3d %4d %5d %3d %7d %8d", k.ma, k.ten, b.nhan, b.mat(), b.tre, b.thieu, b.can, k.biChan, k.catNgon));
        }
    }

    // ---------------------------------------------------------------- Khởi động, dừng
    /** Mở cổng TCP, UDP và chạy các luồng nền: nhận kết nối, nhận UDP, nhịp trộn 20 ms. */
    void batDau(String host, int congTCP, int congUDP) throws IOException {
        this.congUDP = congUDP;
        ln = new ServerSocket();
        ln.setReuseAddress(true);
        ln.bind(new InetSocketAddress(host, congTCP));
        try {
            udp = new DatagramSocket(new InetSocketAddress(host, congUDP));
        } catch (IOException e) {
            ln.close();
            throw e;
        }
        slog("Điều khiển: TCP " + host + ":" + congTCP + "; âm thanh RTP: UDP " + host + ":" + congUDP);
        slog("PCM 16 bit mono 16 kHz; khung " + AmThanh.KHUNG_MS + " ms = " + AmThanh.MAU_MOI_KHUNG + " mẫu = " + AmThanh.BYTE_MOI_KHUNG + " B");
        Thread.ofPlatform().daemon().start(() -> {
            while (true) {
                try {
                    Socket c = ln.accept();
                    c.setTcpNoDelay(true);                             // tin điều khiển đi ngay
                    Thread.ofVirtual().start(() -> phucVu(c));
                } catch (IOException e) {
                    return;                                            // ln đã đóng (Stop Server)
                }
            }
        });
        Thread.ofPlatform().daemon().start(() -> nhanUDP(udp));
        nhip = Executors.newSingleThreadScheduledExecutor();
        nhip.scheduleAtFixedRate(() -> {                               // nhịp đều, không trôi
            try {
                vongTron(udp);
            } catch (RuntimeException e) {                             // lỗi ở móc micro/loa không được làm dừng nhịp
                slog("lỗi nhịp trộn: " + e);
            }
        }, AmThanh.KHUNG_MS, AmThanh.KHUNG_MS, TimeUnit.MILLISECONDS);
    }

    /** Dừng server (nút Stop Server): đóng cổng, ngắt mọi client, dừng nhịp trộn. */
    void dung() {
        try { ln.close(); } catch (IOException ignored) { }
        udp.close();
        nhip.shutdownNow();
        synchronized (khoa) {
            for (Khach k : dsKhach.values()) try { k.conn.close(); } catch (IOException ignored) { }
        }
    }

    public static void main(String[] args) throws IOException {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int congTCP = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        int congUDP = args.length > 2 ? Integer.parseInt(args[2]) : congTCP + 1;
        ServerAmThanh s = new ServerAmThanh(x -> System.out.println("[server] " + x));
        s.batDau(host, congTCP, congUDP);
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {   // Ctrl+C (hoặc lệnh thoat)
            s.slog("Dừng server (Ctrl+C)");
            synchronized (s.khoa) { s.inTongKet(); }
        }));
        if (AmThanh.chayKichBan(s::lenhServer)) System.exit(0);   // lệnh thoat
        // hết stdin: tiếp tục phục vụ tới khi Ctrl+C (luồng nhịp trộn không phải daemon giữ JVM chạy)
    }
}
