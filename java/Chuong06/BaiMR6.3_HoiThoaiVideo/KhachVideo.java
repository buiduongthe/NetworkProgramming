import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;

/**
 * Bài mở rộng MR6.3 – Client hội thoại video.
 *
 * Vòng gửi theo fps cho phép: lấy khung camera tổng hợp, giảm độ phân giải theo yêu cầu server, nén zlib, chia
 * mảnh, gửi UDP. Luồng nhận ghép mảnh khung lưới của server; lệnh "chup" lưu lưới đang xem thành PNG
 * (javax.imageio), --ghep ghép các ảnh đã chụp thành một ảnh khi kết thúc. JDK không có API camera,
 * nên bản Java chỉ dùng camera tổng hợp.
 *
 * Chạy: java -cp out KhachVideo <tên> <host> <cổng_tcp> [--fps 10] [--thoi-gian 10] [--mat-goi 0.02] [--ghep tệp.png]
 * Lệnh (stdin): tat_camera, bat_camera, nhan_fps <n>, nhan_do_phan_giai <1|2>, chup <nhãn>, thoat.
 */
public class KhachVideo {
    record AnhChup(byte[] rgb, int w, int h, String nhan) {}

    /** Ghép các ảnh chụp thành một ảnh: 2 cột, nhãn dưới mỗi ảnh (giống bản Python, Go). */
    static int[] ghepAnh(String duongDan, List<AnhChup> ds) throws IOException {
        final int cot = 2;
        int oW = 0;
        for (AnhChup a : ds) oW = Math.max(oW, a.w());
        oW += 16;
        List<Integer> caoHang = new ArrayList<>();
        for (int i = 0; i < ds.size(); i += cot) {
            int h = 0;
            for (AnhChup a : ds.subList(i, Math.min(ds.size(), i + cot))) h = Math.max(h, a.h());
            caoHang.add(h + 34);
        }
        int W = cot * oW + 16, H = 8;
        for (int h : caoHang) H += h;
        byte[] anh = new byte[W * H * 3];
        Arrays.fill(anh, (byte) 255);
        for (int n = 0; n < ds.size(); n++) {
            AnhChup a = ds.get(n);
            int x0 = 16 + (n % cot) * oW + (oW - 16 - a.w()) / 2, y0 = 8;
            for (int h : caoHang.subList(0, n / cot)) y0 += h;
            for (int y = 0; y < a.h(); y++) System.arraycopy(a.rgb(), y * a.w() * 3, anh, ((y0 + y) * W + x0) * 3, a.w() * 3);
            String nhan = a.nhan().length() > 52 ? a.nhan().substring(0, 52) : a.nhan();
            Video.veChu(anh, W, H, 16 + (n % cot) * oW, y0 + a.h() + 6, nhan, 1, new int[] {0, 0, 0});
        }
        Video.ghiPNG(duongDan, anh, W, H);
        return new int[] {W, H};
    }

    static final Object khoa = new Object();
    static boolean mo = false, dung = false;
    static int fpsServer = Video.FPS_MAC_DINH, chia = 1;
    static AnhChup xem = null;                                 // khung lưới mới nhất
    static int khungNhan = 0, byteNhan = 0;

    public static void main(String[] args) throws Exception {
        String ten = args[0], host = args[1];
        int cong = Integer.parseInt(args[2]);
        int fpsCam = Video.FPS_MAC_DINH;
        double thoiGian = 10, matGoi = 0;
        String ghepRa = "";
        for (int i = 3; i < args.length; i++) {
            switch (args[i]) {
                case "--fps" -> fpsCam = Integer.parseInt(args[++i]);
                case "--thoi-gian" -> thoiGian = Double.parseDouble(args[++i]);
                case "--mat-goi" -> matGoi = Double.parseDouble(args[++i]);
                case "--ghep" -> ghepRa = args[++i];
                default -> { System.err.println("Tham số lạ: " + args[i]); System.exit(2); }
            }
        }
        String tag = "[khach " + ten + "]";
        java.util.function.Consumer<String> log = s -> System.out.println(tag + " " + s);

        // ---- Kênh điều khiển TCP
        Socket tcp = new Socket();
        try {
            tcp.connect(new InetSocketAddress(host, cong), 5000);
        } catch (IOException e) {
            log.accept("Không kết nối được: " + e.getMessage());
            System.exit(1);
        }
        tcp.setTcpNoDelay(true);
        BufferedReader sc = new BufferedReader(new InputStreamReader(tcp.getInputStream(), StandardCharsets.UTF_8));
        Video.guiJSON(tcp, Json.obj("loai", "vao", "ten", ten));
        Map<String, Object> chao = Video.docJSON(String.valueOf(sc.readLine()));
        if (chao == null || !(chao.get("ma") instanceof Number)) {
            log.accept("Server không chào");
            System.exit(1);
        }
        int ma = Video.soNguyen(chao.get("ma"), 0), congUDP = Video.soNguyen(chao.get("cong_udp"), 0);
        Video.CameraTongHop camera = new Video.CameraTongHop(ten, ma);
        log.accept("Vào phòng, mã " + ma + "; camera tổng hợp " + Video.RONG + "x" + Video.CAO + ", " + fpsCam + " fps");

        DatagramSocket udp = new DatagramSocket();
        udp.setReceiveBufferSize(1 << 20);
        udp.connect(new InetSocketAddress(host, congUDP));        // chỉ nhận datagram từ server
        Video.GhepKhung ghep = new Video.GhepKhung();
        List<AnhChup> dsChup = new ArrayList<>();

        Thread.ofPlatform().daemon().start(() -> {                 // đọc kênh điều khiển
            try {
                String line;
                while ((line = sc.readLine()) != null) {
                    Map<String, Object> tin = Video.docJSON(line);
                    if (tin == null) break;
                    Object loai = tin.get("loai");
                    if ("camera".equals(loai)) {
                        synchronized (khoa) {
                            mo = Boolean.TRUE.equals(tin.get("mo"));
                            fpsServer = Video.soNguyen(tin.get("fps"), fpsServer);
                            chia = Video.soNguyen(tin.get("chia"), chia);
                        }
                        log.accept("Camera: " + tin.get("ly_do") + ", gửi tối đa " + Video.soNguyen(tin.get("fps"), 0) + " fps, 1/" + Video.soNguyen(tin.get("chia"), 1));
                    } else if ("nhan".equals(loai)) {
                        log.accept("Nhận: " + Video.soNguyen(tin.get("fps"), 0) + " fps, độ phân giải 1/" + Video.soNguyen(tin.get("chia"), 1));
                    } else if ("su_kien".equals(loai)) {
                        log.accept("Sự kiện: " + tin.get("noi_dung"));
                    } else if ("tu_choi".equals(loai)) {
                        log.accept("Bị từ chối: " + tin.get("ly_do"));
                    }
                }
            } catch (IOException ignored) { }
            synchronized (khoa) { dung = true; }
        });

        Thread.ofPlatform().daemon().start(() -> {                 // nhận và ghép khung lưới
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
                synchronized (khoa) { nen = ghep.them(m.h(), m.payload(), System.nanoTime()); }
                if (nen == null) continue;
                byte[] anh = Video.giaiNen(nen);
                if (anh == null || anh.length != m.h().w() * m.h().h() * 3) continue;
                synchronized (khoa) {
                    xem = new AnhChup(anh, m.h().w(), m.h().h(), "");
                    khungNhan++;
                    byteNhan += nen.length;
                }
            }
        });

        Thread.ofPlatform().daemon().start(() -> Video.chayKichBan(dong -> {
            String[] p = dong.split("\\s+");
            log.accept("> " + dong);
            try {
                if (p[0].equals("tat_camera")) {                   // tự tắt: có hiệu lực ngay
                    synchronized (khoa) { mo = false; }
                    Video.guiJSON(tcp, Json.obj("loai", "tu_tat_camera"));
                } else if (p[0].equals("bat_camera")) {            // tự bật: chờ server cho phép
                    Video.guiJSON(tcp, Json.obj("loai", "tu_bat_camera"));
                } else if (p[0].equals("nhan_fps") && p.length == 2) {
                    Video.guiJSON(tcp, Json.obj("loai", "nhan", "fps", Integer.parseInt(p[1])));
                } else if (p[0].equals("nhan_do_phan_giai") && p.length == 2) {
                    Video.guiJSON(tcp, Json.obj("loai", "nhan", "chia", Integer.parseInt(p[1])));
                } else if (p[0].equals("chup")) {
                    AnhChup a;
                    synchronized (khoa) { a = xem; }
                    if (a == null) {
                        log.accept("chưa có hình để chụp");
                        return true;
                    }
                    new File("out").mkdirs();
                    String duong = "out/chup_" + (dsChup.size() + 1) + ".png";
                    Video.ghiPNG(duong, a.rgb(), a.w(), a.h());
                    String nhan = String.join(" ", Arrays.asList(p).subList(1, p.length));
                    dsChup.add(new AnhChup(a.rgb(), a.w(), a.h(), nhan.isEmpty() ? duong : nhan));
                    log.accept("Đã chụp " + duong + " (" + a.w() + "x" + a.h() + ")");
                } else if (p[0].equals("thoat")) {
                    synchronized (khoa) { dung = true; }
                } else {
                    log.accept("lệnh không hợp lệ");
                }
            } catch (IOException | NumberFormatException e) {
                log.accept("lỗi: " + e.getMessage());
            }
            return true;
        }));

        // ---- Vòng gửi khung theo fps cho phép; mỗi giây in thống kê
        Random rng = new Random(ma);
        byte[] giu = Video.goiGiu(ma);
        udp.send(new DatagramPacket(giu, giu.length));            // đăng ký địa chỉ UDP với server
        int soKhung = 0, guiKhung = 0, guiManh = 0, boGiaLap = 0, demGui = 0, byteGui = 0, giay = 0;
        long hanGui = System.nanoTime(), mocDo = hanGui, hanGiu = mocDo + 1_000_000_000L;
        log.accept("Mỗi giây: fps và kbit/s gửi | nhận, số mảnh mất");
        while (Video.giay() < thoiGian) {
            long bayGio = System.nanoTime();
            boolean dangMo;
            int fps, d;
            synchronized (khoa) {
                if (dung) break;
                dangMo = mo;
                fps = Math.min(fpsCam, fpsServer);
                d = chia;
            }
            if (dangMo && bayGio >= hanGui) {
                hanGui += 1_000_000_000L / fps;
                if (hanGui < bayGio) hanGui = bayGio;
                Video.Anh a = Video.thuNho(camera.layKhung(soKhung), Video.RONG, Video.CAO, d);   // giảm độ phân giải rồi nén
                byte[] nen = Video.nenZlib(a.rgb());
                for (byte[] g : Video.chiaManh(ma, soKhung, a.w(), a.h(), nen)) {
                    guiManh++;
                    if (rng.nextDouble() < matGoi) boGiaLap++;    // "mạng" làm rơi mảnh
                    else udp.send(new DatagramPacket(g, g.length));
                }
                soKhung++;
                guiKhung++;
                demGui++;
                byteGui += nen.length;
            } else if (!dangMo && bayGio >= hanGiu) {
                udp.send(new DatagramPacket(giu, giu.length));    // giữ kết nối khi tắt camera
                hanGiu = bayGio + 1_000_000_000L;
            }
            if (bayGio - mocDo >= 1_000_000_000L) {
                mocDo += 1_000_000_000L;
                giay++;
                int kn, bn, mat;
                synchronized (khoa) {
                    kn = khungNhan;
                    bn = byteNhan;
                    mat = ghep.manhMat;
                    khungNhan = byteNhan = 0;
                }
                log.accept(String.format(Locale.ROOT, "%2ds gửi %2d fps %4.0f | nhận %2d fps %5.0f, mất %d", giay, demGui,
                        byteGui * 8 / 1000.0, kn, bn * 8 / 1000.0, mat));
                demGui = byteGui = 0;
            }
            Thread.sleep(2);
        }

        // ---- Kết thúc
        tcp.close();
        udp.close();
        log.accept(String.format(Locale.ROOT, "Kết thúc sau %.1f s", Video.giay()));
        log.accept("Gửi " + guiKhung + " khung = " + guiManh + " mảnh, giả lập mất " + boGiaLap + " mảnh");
        synchronized (khoa) {
            log.accept("Nhận " + ghep.khungDu + " khung đủ, hỏng " + ghep.khungHong + ", mất hẳn " + ghep.khungMatHan + ", mảnh mất " + ghep.manhMat);
        }
        if (!ghepRa.isEmpty() && !dsChup.isEmpty()) {
            int[] wh = ghepAnh(ghepRa, dsChup);
            log.accept("Đã ghép " + dsChup.size() + " ảnh chụp vào " + ghepRa + " (" + wh[0] + "x" + wh[1] + ")");
        }
        System.exit(0);
    }
}
