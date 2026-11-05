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
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.TreeMap;

/**
 * Bài mở rộng MR6.3 – Server hội thoại video ghép lưới kiểu MCU (Multipoint Control Unit): giải nén khung của mọi
 * client, ghép thành lưới (mỗi ô 160×120), nén lại, gửi cho từng người nhận theo fps/độ phân giải người đó chọn.
 *
 * Quy tắc quyền điều khiển (giống mic ở MR6.2):
 *  1. Camera của X mở khi và chỉ khi X không tự tắt VÀ server không tắt camera của X.
 *  2. Server tắt thắng client: đang bị server tắt thì client xin bật bị từ chối.
 *  3. Server bật = gỡ lệnh tắt của server, KHÔNG bật hộ camera mà người dùng tự tắt.
 *  4. Giới hạn gửi của từng client (fps tối đa, độ phân giải 1/1, 1/2, 1/4) và fps chung do server đặt.
 *  5. Tham số nhận (fps, độ phân giải lưới) do từng client tự chọn, không vượt fps chung.
 *  6. Server tự thực thi: bỏ khung của camera đang tắt, khung đến nhanh hơn fps cho phép, datagram sai địa chỉ nguồn.
 *
 * Chạy: java -cp out ServerVideo [host] [cổng_tcp] [cổng_udp]
 * Lệnh (stdin): tat_camera <tên|tat_ca>, bat_camera <tên|tat_ca>, fps_toi_da <tên|tat_ca> <n>,
 *               do_phan_giai <tên|tat_ca> <1|2|4>, fps_chung <n>, trang_thai, thoat; "@t lệnh" = chờ tới giây t.
 */
public class ServerVideo {
    static final class Khach {
        final int ma;
        final String ten, ip;
        final Socket conn;
        SocketAddress udp;
        boolean tuTat, serverTat;
        int fpsToiDa = Video.FPS_MAC_DINH, chia = 1;       // giới hạn gửi do server đặt
        int nhanFps = Video.FPS_MAC_DINH, nhanChia = 1;    // tham số nhận do client chọn
        final Video.GhepKhung ghep = new Video.GhepKhung();
        byte[] o;                                          // ô 160×120 đã phóng to, có nhãn – dùng khi ghép lưới
        long lucNhanCuoi = Long.MIN_VALUE / 2;
        int boVuotFps, boTat, saiNguon;
        int demKhung, demByte;                             // cửa sổ 1 giây để đo fps, kbit/s vào
        double fpsVao, kbpsVao;
        long soKhungRa;
        long hanGuiRa = 0;
        int demRa, byteRa;
        double fpsRa, kbpsRa;

        Khach(int ma, String ten, String ip, Socket conn) { this.ma = ma; this.ten = ten; this.ip = ip; this.conn = conn; }

        boolean camMo() { return !tuTat && !serverTat; }   // quy tắc 1

        String lyDo() {
            if (camMo()) return "mở";
            if (tuTat && serverTat) return "tắt (cả hai)";
            return serverTat ? "tắt (server)" : "tắt (tự tắt)";
        }

        void gui(Map<String, Object> v) { Video.guiJSON(conn, v); }

        /** Báo cho client tham số gửi hiện hành. */
        void guiCamera() {
            gui(Json.obj("loai", "camera", "mo", camMo(), "ly_do", lyDo(), "fps", fpsToiDa, "chia", chia));
            veO(null, 0, 0);
        }

        void guiNhan() { gui(Json.obj("loai", "nhan", "fps", Math.min(nhanFps, fpsChung), "chia", nhanChia)); }

        /** Vẽ ô của client trong lưới: hình đã phóng về 160×120 + dải nhãn trạng thái. */
        void veO(byte[] anh, int w, int h) {
            byte[] moi;
            if (anh != null) moi = Video.phongTo(anh, w, h, Video.RONG / w);
            else if (camMo() && o != null) moi = o;
            else {
                moi = new byte[Video.RONG * Video.CAO * 3];
                Video.toMau(moi, Video.RONG, 0, 0, Video.RONG, Video.CAO, new int[] {40, 40, 40});
                if (!camMo()) {
                    int[] trang = {255, 255, 255};
                    Video.veChu(moi, Video.RONG, Video.CAO, 20, 44, "TAT CAMERA", 2, trang);
                    Video.veChu(moi, Video.RONG, Video.CAO, 44, 66, serverTat ? "(SERVER)" : "(TU TAT)", 2, trang);
                }
            }
            Video.toMau(moi, Video.RONG, 0, Video.CAO - 11, Video.RONG, 11, new int[] {0, 0, 0});
            String nhan = ten + " " + fpsToiDa + "FPS" + (chia > 1 ? " 1/" + chia : "");
            Video.veChu(moi, Video.RONG, Video.CAO, 2, Video.CAO - 9, nhan, 1, new int[] {255, 255, 0});
            o = moi;
        }
    }

    static final Object khoa = new Object();
    static final TreeMap<Integer, Khach> dsKhach = new TreeMap<>();
    static final List<Khach> daRoi = new ArrayList<>();
    static int maMoi = 0;
    static int fpsChung = Video.FPS_MAC_DINH;              // giới hạn fps đầu ra chung cho mọi người nhận
    static int congUDP;

    static void slog(String s) { System.out.println("[server] " + s); }

    static void phatSuKien(String text) {
        slog("  => " + text);
        for (Khach k : dsKhach.values()) k.gui(Json.obj("loai", "su_kien", "noi_dung", text));
    }

    // ---------------------------------------------------------------- Kênh điều khiển TCP
    static void phucVu(Socket c) {
        try (c) {
            BufferedReader r = new BufferedReader(new InputStreamReader(c.getInputStream(), StandardCharsets.UTF_8));
            Map<String, Object> tin = Video.docJSON(String.valueOf(r.readLine()));
            if (tin == null || !"vao".equals(tin.get("loai")) || !(tin.get("ten") instanceof String ten) || ten.isEmpty()) return;
            if (ten.length() > 8) ten = ten.substring(0, 8);
            Khach k;
            synchronized (khoa) {
                k = new Khach(++maMoi, ten, c.getInetAddress().getHostAddress(), c);
                dsKhach.put(k.ma, k);
            }
            k.gui(Json.obj("loai", "chao", "ma", k.ma, "cong_udp", congUDP, "rong", Video.RONG, "cao", Video.CAO));
            synchronized (khoa) {
                slog("#" + k.ma + " " + k.ten + " vào phòng từ " + k.ip + ":" + c.getPort());
                k.guiCamera();
                k.guiNhan();
                phatSuKien(k.ten + " vào phòng");
            }
            String line;
            while ((line = r.readLine()) != null) {
                Map<String, Object> t = Video.docJSON(line);
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

    static void xuLyTinClient(Khach k, Map<String, Object> tin) {
        Object loai = tin.get("loai");
        if ("tu_tat_camera".equals(loai)) {
            k.tuTat = true;
            k.guiCamera();
            phatSuKien(k.ten + " tự tắt camera");
        } else if ("tu_bat_camera".equals(loai)) {
            k.tuTat = false;
            if (k.serverTat) {                                     // quy tắc 2
                slog(k.ten + " xin bật camera -> từ chối (server đang tắt)");
                k.gui(Json.obj("loai", "tu_choi", "ly_do", "server đang tắt camera của bạn"));
            } else {
                k.guiCamera();
                phatSuKien(k.ten + " tự bật camera");
            }
        } else if ("nhan".equals(loai)) {                          // quy tắc 5
            k.nhanFps = Math.max(1, Math.min(30, Video.soNguyen(tin.get("fps"), k.nhanFps)));
            k.nhanChia = Video.soNguyen(tin.get("chia"), k.nhanChia) == 2 ? 2 : 1;
            slog(k.ten + " chọn nhận " + k.nhanFps + " fps, độ phân giải 1/" + k.nhanChia);
            k.guiNhan();
        }
    }

    // ---------------------------------------------------------------- Kênh dữ liệu UDP
    static void nhanUDP(DatagramSocket sock) {
        byte[] buf = new byte[2048];
        while (true) {
            DatagramPacket p = new DatagramPacket(buf, buf.length);
            try {
                sock.receive(p);
            } catch (IOException e) {
                return;
            }
            Video.Manh m = Video.tachHeader(p.getData(), p.getLength());
            if (m == null) continue;
            long bayGio = System.nanoTime();
            synchronized (khoa) { xuLyManh(m, p, bayGio); }
        }
    }

    static void xuLyManh(Video.Manh m, DatagramPacket p, long bayGio) {
        Video.Header h = m.h();
        Khach k = dsKhach.get(h.ma());
        if (k == null) return;
        if (k.udp == null && p.getAddress().getHostAddress().equals(k.ip)) {
            k.udp = p.getSocketAddress();
            slog("#" + k.ma + " " + k.ten + " đăng ký UDP " + p.getAddress().getHostAddress() + ":" + p.getPort());
        }
        if (k.udp == null || !p.getSocketAddress().equals(k.udp)) {   // quy tắc 6
            k.saiNguon++;
            return;
        }
        if (h.loai() != Video.LOAI_MANH) return;
        byte[] nen = k.ghep.them(h, m.payload(), bayGio);
        if (nen == null) return;
        if (!k.camMo()) {                                          // quy tắc 6: camera tắt
            k.boTat++;
            return;
        }
        if ((bayGio - k.lucNhanCuoi) / 1e9 < 0.8 / k.fpsToiDa) {   // vượt fps (dung sai 20%)
            k.boVuotFps++;
            return;
        }
        byte[] anh = Video.giaiNen(nen);
        int w = h.w(), c = h.h();
        if (anh == null || w == 0 || w * c * 3 != anh.length || Video.RONG % w != 0 || w * Video.CAO != c * Video.RONG) return;   // kích thước không hợp lệ
        k.lucNhanCuoi = bayGio;
        k.demKhung++;
        k.demByte += nen.length;
        k.veO(anh, w, c);
    }

    // ---------------------------------------------------------------- Ghép lưới và gửi

    /** Ghép ô của mọi client thành lưới cot × hang (cot = ⌈√n⌉). */
    static Video.Anh ghepLuoi(List<Khach> ds) {
        int n = Math.max(1, ds.size());
        int cot = (int) Math.ceil(Math.sqrt(n)), hang = (n + cot - 1) / cot;
        int W = cot * Video.RONG, H = hang * Video.CAO;
        byte[] ra = new byte[W * H * 3];
        Video.toMau(ra, W, 0, 0, W, H, new int[] {15, 15, 15});
        for (int i = 0; i < ds.size(); i++) {
            int x0 = (i % cot) * Video.RONG, y0 = (i / cot) * Video.CAO;
            for (int y = 0; y < Video.CAO; y++)
                System.arraycopy(ds.get(i).o, y * Video.RONG * 3, ra, ((y0 + y) * W + x0) * 3, Video.RONG * 3);
        }
        return new Video.Anh(ra, W, H);
    }

    /**
     * Mỗi 5 ms kiểm tra người nhận nào tới hạn (theo fps nhận) thì ghép lưới, thu nhỏ theo độ phân giải nhận, nén,
     * chia mảnh và gửi. Mỗi giây tính lại fps, kbit/s.
     */
    static void vongGui(DatagramSocket sock) {
        long mocDo = System.nanoTime();
        while (true) {
            long bayGio = System.nanoTime();
            synchronized (khoa) {
                List<Khach> ds = new ArrayList<>();
                for (Khach k : dsKhach.values()) if (k.o != null) ds.add(k);
                for (Khach k : dsKhach.values()) {
                    int fps = Math.min(k.nhanFps, fpsChung);
                    if (k.udp == null || ds.isEmpty() || bayGio < k.hanGuiRa) continue;
                    k.hanGuiRa += 1_000_000_000L / fps;
                    if (k.hanGuiRa < bayGio) k.hanGuiRa = bayGio;
                    Video.Anh luoi = ghepLuoi(ds);
                    Video.Anh a = Video.thuNho(luoi.rgb(), luoi.w(), luoi.h(), k.nhanChia);
                    byte[] nen = Video.nenZlib(a.rgb());
                    for (byte[] g : Video.chiaManh(Video.MA_SERVER, k.soKhungRa, a.w(), a.h(), nen)) {
                        try {
                            sock.send(new DatagramPacket(g, g.length, k.udp));
                        } catch (IOException ignored) { }
                    }
                    k.soKhungRa++;
                    k.demRa++;
                    k.byteRa += nen.length;
                }
                if (bayGio - mocDo >= 1_000_000_000L) {
                    for (Khach k : dsKhach.values()) {
                        k.fpsVao = k.demKhung;
                        k.kbpsVao = k.demByte * 8 / 1000.0;
                        k.fpsRa = k.demRa;
                        k.kbpsRa = k.byteRa * 8 / 1000.0;
                        k.demKhung = k.demByte = k.demRa = k.byteRa = 0;
                    }
                    mocDo += 1_000_000_000L;
                }
            }
            try { Thread.sleep(5); } catch (InterruptedException e) { return; }
        }
    }

    // ---------------------------------------------------------------- Bảng điều khiển
    static List<Khach> chon(String ten) {
        List<Khach> kq = new ArrayList<>();
        for (Khach k : dsKhach.values()) if (ten.equals("tat_ca") || k.ten.equals(ten)) kq.add(k);
        return kq;
    }

    static boolean lenhServer(String dong) {
        String[] p = dong.strip().split("\\s+");
        if (p[0].isEmpty()) return true;
        slog(String.format(Locale.ROOT, "t=%.1fs > %s", Video.giay(), dong));
        synchronized (khoa) {
            String ai = p.length > 1 ? (p[1].equals("tat_ca") ? "tất cả" : p[1]) : "";
            if ((p[0].equals("tat_camera") || p[0].equals("bat_camera")) && p.length == 2) {
                List<String> vanTat = new ArrayList<>();
                for (Khach k : chon(p[1])) {
                    k.serverTat = p[0].equals("tat_camera");
                    k.guiCamera();
                    if (k.tuTat) vanTat.add(k.ten);
                }
                if (p[0].equals("tat_camera")) phatSuKien("server tắt camera " + ai);
                else phatSuKien("server bật camera " + ai + (vanTat.isEmpty() ? "" : " (" + String.join(", ", vanTat) + " vẫn tự tắt)"));   // quy tắc 3
            } else if (p[0].equals("fps_toi_da") && p.length == 3) {
                int n = Integer.parseInt(p[2]);
                for (Khach k : chon(p[1])) {
                    k.fpsToiDa = Math.max(1, Math.min(30, n));
                    k.guiCamera();
                }
                phatSuKien("server giới hạn " + ai + " gửi tối đa " + p[2] + " fps");
            } else if (p[0].equals("do_phan_giai") && p.length == 3 && List.of("1", "2", "4").contains(p[2])) {
                int n = Integer.parseInt(p[2]);
                for (Khach k : chon(p[1])) {
                    k.chia = n;
                    k.guiCamera();
                }
                phatSuKien("server đặt độ phân giải " + ai + " = 1/" + p[2]);
            } else if (p[0].equals("fps_chung") && p.length == 2) {
                fpsChung = Math.max(1, Math.min(30, Integer.parseInt(p[1])));
                for (Khach k : dsKhach.values()) k.guiNhan();
                phatSuKien("server đặt fps chung tối đa " + fpsChung);
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

    static void inTrangThai() {
        slog("Trạng thái: " + dsKhach.size() + " client, fps chung " + fpsChung);
        slog("                     giới hạn vào        ra (lưới)");
        slog(" #  tên camera       fps 1/n  fps  kbit  fps  kbit");
        for (Khach k : dsKhach.values())
            slog(String.format(Locale.ROOT, "%2d  %-3s %s %3d %3d %4.0f %5.0f %4.0f %5.0f", k.ma, k.ten, padPhai(k.lyDo(), 12),
                    k.fpsToiDa, k.chia, k.fpsVao, k.kbpsVao, k.fpsRa, k.kbpsRa));
    }

    static void inTongKet() {
        slog("Tổng kết luồng vào (từ client tới server):");
        slog(" #  tên mảnh khung hỏng mảnh mất  bỏ:tắt vượt fps");
        slog("        nhận    đủ        mất hẳn");
        List<Khach> ds = new ArrayList<>(dsKhach.values());
        ds.addAll(daRoi);
        ds.sort((a, b) -> Integer.compare(a.ma, b.ma));
        for (Khach k : ds) {
            Video.GhepKhung g = k.ghep;
            slog(String.format("%2d  %-3s %4d %5d %4d %4d %4d %6d %6d", k.ma, k.ten, g.manhNhan, g.khungDu, g.khungHong,
                    g.manhMat, g.khungMatHan, k.boTat, k.boVuotFps));
        }
    }

    public static void main(String[] args) throws IOException {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int congTCP = args.length > 1 ? Integer.parseInt(args[1]) : 9000;
        congUDP = args.length > 2 ? Integer.parseInt(args[2]) : congTCP + 1;
        ServerSocket ln = new ServerSocket();
        ln.setReuseAddress(true);
        ln.bind(new InetSocketAddress(host, congTCP));
        DatagramSocket udp = new DatagramSocket(new InetSocketAddress(host, congUDP));
        udp.setReceiveBufferSize(1 << 20);                         // đệm nhận lớn: mảnh đến dồn dập
        slog("TCP " + host + ":" + congTCP + " điều khiển; UDP :" + congUDP + " video");
        slog("Khung " + Video.RONG + "x" + Video.CAO + " RGB " + Video.RONG * Video.CAO * 3 + " B, nén zlib, mảnh ≤ 1200 B");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {
            slog("Dừng server (Ctrl+C)");
            synchronized (khoa) { inTongKet(); }
        }));
        Thread.ofPlatform().daemon().start(() -> {
            while (true) {
                try {
                    Socket c = ln.accept();
                    c.setTcpNoDelay(true);
                    Thread.ofVirtual().start(() -> phucVu(c));
                } catch (IOException e) {
                    return;
                }
            }
        });
        Thread.ofPlatform().daemon().start(() -> nhanUDP(udp));
        Thread.ofPlatform().start(() -> vongGui(udp));             // luồng thường: giữ JVM chạy tới khi Ctrl+C
        if (Video.chayKichBan(ServerVideo::lenhServer)) System.exit(0);
    }
}
