import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStream;
import java.io.InputStreamReader;
import java.io.OutputStream;
import java.io.RandomAccessFile;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.nio.file.Path;
import java.nio.file.StandardCopyOption;
import java.security.SecureRandom;
import java.util.ArrayList;
import java.util.Comparator;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.function.BooleanSupplier;

/**
 * Bài mở rộng MR5.1 – Client truyền nhiều tệp đồng thời, có điều khiển start/pause/resume/stop,
 * giới hạn tốc độ theo phiên và theo tệp (thùng token), tải tiếp bằng offset + kiểm SHA-256.
 *
 * Vai:
 *   client <host> <port> [thư_mục_tải]   – giao diện dòng lệnh: nhập lệnh qua bàn phím.
 *   demo   <host> <port>                 – tự dựng server cục bộ, tạo tệp thử, chạy kịch bản tự động.
 *
 * Lệnh của giao diện client (mỗi dòng một lệnh):
 *   start <tên> [download|upload] [tốc_độ]   bắt đầu một lượt (mặc định download); tốc_độ = giới hạn tệp
 *   pause <tên>        tạm dừng (giữ tệp .part)        resume <tên>   tải tiếp từ offset
 *   stop  <tên>        dừng hẳn (xóa tệp .part)         speed <tốc_độ> đổi giới hạn tốc độ phiên
 *   status             xem tiến trình                   quit           thoát
 * Đơn vị tốc độ: 512K, 1M, 2MiB, hoặc số byte/giây.
 *
 * Chạy: java -cp out TruyenClient client|demo <host> <port> [thư_mục_tải]
 */
public class TruyenClient {
    static final String CHAY = "chạy", TAM_DUNG = "tạm dừng", DUNG = "dừng", XONG = "xong", LOI = "lỗi";

    /** Một lượt truyền (một tệp). Chạy trong luồng riêng, điều khiển qua trường trangThai (volatile). */
    static final class LuotTruyen {
        final String ten, huong, cucBo;                // huong: "download" | "upload"; cucBo: đường tệp phía client
        volatile String trangThai = CHAY;
        volatile long tong = 0, xongByte = 0;
        volatile long batDauTu = 0;                    // offset lúc bắt đầu lượt (tải tiếp: khác 0)
        String shaServer;
        volatile Boolean shaOk = null;
        volatile long t0 = System.nanoTime();
        double thoiGian;
        final Khung.ThungToken bucket;                 // giới hạn riêng của tệp
        Thread luong;

        LuotTruyen(String ten, String huong, String cucBo, double rateTep) {
            this.ten = ten; this.huong = huong; this.cucBo = cucBo;
            this.bucket = new Khung.ThungToken(rateTep);
        }

        boolean dungKhi() { return trangThai.equals(TAM_DUNG) || trangThai.equals(DUNG); }
    }

    static double giay(long t0) { return (System.nanoTime() - t0) / 1e9; }

    static String thanhTienDo(double pct) {
        int day = (int) (pct / 100 * 20);
        return "[" + "#".repeat(day) + "-".repeat(20 - day) + "]";
    }

    static final class QuanLy {
        final String host, thuMuc, khoServer;          // khoServer: để suy ra offset khi upload tiếp
        final int port;
        Khung.ThungToken session = new Khung.ThungToken(Khung.KHONG_GIOI_HAN);
        final Map<String, LuotTruyen> luot = new LinkedHashMap<>();

        QuanLy(String host, int port, String thuMucTai, String khoServer) throws IOException {
            this.host = host; this.port = port; this.thuMuc = thuMucTai; this.khoServer = khoServer;
            Files.createDirectories(Path.of(thuMucTai));
        }

        // ------------- Điều khiển -------------
        void datTocDoPhien(double rate) {
            session.datRate(rate);
            System.out.println("[client] Đặt tốc độ phiên = " + Khung.dinhDangByte((long) rate) + "/s");
        }

        synchronized void batDau(String ten, String huong, double rateTep) {
            LuotTruyen cu = luot.get(ten);
            if (cu != null && cu.trangThai.equals(CHAY)) {
                System.out.println("[client] " + ten + " đang chạy");
                return;
            }
            LuotTruyen t = new LuotTruyen(ten, huong, new File(thuMuc, ten).getPath(), rateTep);
            luot.put(ten, t);
            t.luong = new Thread(() -> { if (huong.equals("download")) taiXuong(t); else taiLen(t); });
            t.luong.setDaemon(true);
            t.t0 = System.nanoTime();
            t.luong.start();
            String gh = rateTep >= Khung.KHONG_GIOI_HAN ? "" : ", giới hạn tệp " + Khung.dinhDangByte((long) rateTep) + "/s";
            System.out.println("[client] BẮT ĐẦU " + huong + " " + ten + gh);
        }

        void tamDung(String ten) {
            LuotTruyen t = luot.get(ten);
            if (t != null && t.trangThai.equals(CHAY)) {
                t.trangThai = TAM_DUNG;
                System.out.println("[client] TẠM DỪNG " + ten + " (giữ .part)");
            }
        }

        void taiTiep(String ten) {
            LuotTruyen t = luot.get(ten);
            if (t != null && t.trangThai.equals(TAM_DUNG)) {
                System.out.println("[client] TẢI TIẾP " + ten);
                batDau(ten, t.huong, t.bucket.rate);
            }
        }

        void dungHan(String ten) {
            LuotTruyen t = luot.get(ten);
            if (t != null && (t.trangThai.equals(CHAY) || t.trangThai.equals(TAM_DUNG))) {
                t.trangThai = DUNG;
                System.out.println("[client] DỪNG " + ten + " (xóa .part)");
            }
        }

        void trangThai() {
            System.out.println("[client] --- Trạng thái ---");
            for (LuotTruyen t : new ArrayList<>(luot.values())) {
                double pct = t.tong > 0 ? 100.0 * t.xongByte / t.tong : 0;
                System.out.println(String.format(Locale.ROOT, "[client]   %-12s %-8s %-8s %s %5.1f%%  %s/%s", t.ten, t.huong,
                        t.trangThai, thanhTienDo(pct), pct, Khung.dinhDangByte(t.xongByte), Khung.dinhDangByte(t.tong)));
            }
        }

        // ------------- Thân luồng truyền -------------
        void taiXuong(LuotTruyen t) {
            File part = new File(t.cucBo + ".part");
            long off = part.exists() ? part.length() : 0;
            try (Socket s = new Socket()) {
                s.connect(new InetSocketAddress(host, port));
                Khung.guiKhung(s, Json.obj("op", "download", "name", t.ten, "offset", off));
                InputStream in = s.getInputStream();
                Map<String, Object> resp = Khung.nhanKhung(in);
                if (!Boolean.TRUE.equals(resp.get("ok"))) {
                    t.trangThai = LOI;
                    System.out.println("[client] " + t.ten + ": lỗi " + resp.get("loi"));
                    return;
                }
                t.tong = ((Number) resp.get("size")).longValue();
                t.shaServer = (String) resp.get("sha256");
                t.xongByte = off;
                t.batDauTu = off;
                if (off >= t.tong) {                       // đã đủ: chỉ cần kiểm và đổi tên
                    s.close();
                    hoanTatTaiXuong(t, part);
                    return;
                }
                byte[] buf = new byte[Khung.CHUNK];
                try (RandomAccessFile f = new RandomAccessFile(part, "rw")) {
                    if (off == 0) f.setLength(0);
                    f.seek(off);
                    while (t.xongByte < t.tong) {
                        if (t.trangThai.equals(DUNG)) {
                            f.close();
                            s.close();
                            Files.deleteIfExists(part.toPath());
                            return;
                        }
                        if (t.trangThai.equals(TAM_DUNG)) return;   // try-with-resources đóng tệp và socket
                        int n = in.read(buf);
                        if (n == -1) break;
                        f.write(buf, 0, n);
                        t.xongByte += n;
                        session.tieuThu(n, t::dungKhi);        // điều tiết theo phiên
                        t.bucket.tieuThu(n, t::dungKhi);       // và theo tệp
                    }
                }
                s.close();
                if (t.xongByte >= t.tong) hoanTatTaiXuong(t, part);
            } catch (IOException | InterruptedException e) {
                if (!t.dungKhi()) {
                    t.trangThai = LOI;
                    System.out.println("[client] " + t.ten + ": lỗi kết nối " + e.getMessage());
                }
            }
        }

        void hoanTatTaiXuong(LuotTruyen t, File part) throws IOException {
            t.shaOk = Khung.sha256Tep(part.getPath()).equals(t.shaServer);
            Files.move(part.toPath(), Path.of(t.cucBo), StandardCopyOption.REPLACE_EXISTING);
            t.thoiGian = giay(t.t0);
            t.trangThai = XONG;
            System.out.println(String.format(Locale.ROOT, "[client] XONG download %s: %s trong %.1fs (%s/s), SHA-256 %s", t.ten,
                    Khung.dinhDangByte(t.tong), t.thoiGian, Khung.dinhDangByte((long) (t.tong / Math.max(t.thoiGian, 0.001))),
                    t.shaOk ? "KHỚP" : "SAI"));
        }

        void taiLen(LuotTruyen t) {
            try {
                long total = new File(t.cucBo).length();
                File partServer = new File(khoServer, t.ten + ".part");
                long off = partServer.exists() ? partServer.length() : 0;
                String sha = Khung.sha256Tep(t.cucBo);
                try (Socket s = new Socket(); RandomAccessFile f = new RandomAccessFile(t.cucBo, "r")) {
                    s.connect(new InetSocketAddress(host, port));
                    Khung.guiKhung(s, Json.obj("op", "upload", "name", t.ten, "offset", off, "size", total, "sha256", sha));
                    InputStream in = s.getInputStream();
                    OutputStream out = s.getOutputStream();
                    Map<String, Object> resp = Khung.nhanKhung(in);
                    if (!Boolean.TRUE.equals(resp.get("ok"))) {
                        t.trangThai = LOI;
                        return;
                    }
                    t.tong = total;
                    t.xongByte = off;
                    t.batDauTu = off;
                    f.seek(off);
                    byte[] buf = new byte[Khung.CHUNK];
                    while (t.xongByte < total) {
                        if (t.dungKhi()) return;
                        int n = f.read(buf);
                        if (n <= 0) break;
                        session.tieuThu(n, t::dungKhi);
                        t.bucket.tieuThu(n, t::dungKhi);
                        out.write(buf, 0, n);
                        t.xongByte += n;
                    }
                    Map<String, Object> ket = Khung.nhanKhung(in);
                    t.thoiGian = giay(t.t0);
                    t.shaOk = Boolean.TRUE.equals(ket.get("sha256_ok"));
                    t.trangThai = XONG;
                    System.out.println(String.format(Locale.ROOT, "[client] XONG upload %s: %s trong %.1fs (%s/s), server kiểm SHA-256 %s",
                            t.ten, Khung.dinhDangByte(total), t.thoiGian, Khung.dinhDangByte((long) (total / Math.max(t.thoiGian, 0.001))),
                            t.shaOk ? "KHỚP" : "SAI"));
                }
            } catch (IOException | InterruptedException e) {
                if (!t.dungKhi()) {
                    t.trangThai = LOI;
                    System.out.println("[client] " + t.ten + ": lỗi kết nối " + e.getMessage());
                }
            }
        }
    }

    // ----------------------------- Vai: giao diện dòng lệnh -----------------------------
    static void chayGiaoDien(String host, int port, String thuMuc) throws IOException {
        QuanLy ql = new QuanLy(host, port, thuMuc, "kho_server");
        System.out.println("[client] Nhập lệnh (start/pause/resume/stop/speed/status/quit).");
        BufferedReader stdin = new BufferedReader(new InputStreamReader(System.in, StandardCharsets.UTF_8));
        String dong;
        while ((dong = stdin.readLine()) != null) {
            String[] p = dong.strip().split("\\s+");
            if (p[0].isEmpty()) continue;
            String lenh = p[0].toLowerCase(Locale.ROOT);
            try {
                switch (lenh) {
                    case "start" -> ql.batDau(p[1], p.length > 2 ? p[2] : "download", p.length > 3 ? Khung.docSoByte(p[3]) : Khung.KHONG_GIOI_HAN);
                    case "pause" -> ql.tamDung(p[1]);
                    case "resume" -> ql.taiTiep(p[1]);
                    case "stop" -> ql.dungHan(p[1]);
                    case "speed" -> ql.datTocDoPhien(Khung.docSoByte(p[1]));
                    case "status" -> ql.trangThai();
                    case "quit", "exit" -> { return; }
                    default -> System.out.println("[client] Lệnh lạ");
                }
            } catch (ArrayIndexOutOfBoundsException e) {
                System.out.println("[client] Thiếu tham số");
            }
        }
    }

    // ----------------------------- Vai: demo tự động -----------------------------
    static boolean choDen(BooleanSupplier dieuKien, double han) throws InterruptedException {
        long het = System.nanoTime() + (long) (han * 1e9);
        while (System.nanoTime() < het) {
            if (dieuKien.getAsBoolean()) return true;
            Thread.sleep(100);
        }
        return false;
    }

    static void xoaCay(File d) {
        File[] con = d.listFiles();
        if (con != null) for (File c : con) xoaCay(c);
        d.delete();
    }

    static void chayDemo(String host, int port) throws Exception {
        File goc = Files.createTempDirectory("mr51_").toFile();
        File kho = new File(goc, "kho_server"), tai = new File(goc, "tai_ve"), nguon = new File(goc, "nguon");
        for (File d : new File[] {kho, tai, nguon}) d.mkdirs();

        // Server cục bộ trong một luồng (cùng mã với vai server).
        ServerSocket srv = new ServerSocket();
        srv.setReuseAddress(true);
        srv.bind(new InetSocketAddress(host, port), 16);
        Thread vongServer = new Thread(() -> {
            while (true) {
                Socket conn;
                try {
                    conn = srv.accept();
                } catch (IOException e) {
                    return;
                }
                Thread t = new Thread(() -> TruyenServer.phucVu(conn, kho.getPath()));
                t.setDaemon(true);
                t.start();
            }
        });
        vongServer.setDaemon(true);
        vongServer.start();
        System.out.println("[demo] Server cục bộ tại " + host + ":" + port + ", thư mục tạm " + goc);

        // Tạo 3 tệp thử (3 MiB mỗi tệp) trực tiếp trong kho server để tải xuống.
        List<String> cacTep = List.of("tep1.bin", "tep2.bin", "tep3.bin");
        SecureRandom rnd = new SecureRandom();
        for (String ten : cacTep) {
            byte[] data = new byte[3 * 1024 * 1024];
            rnd.nextBytes(data);
            Files.write(new File(kho, ten).toPath(), data);
        }
        System.out.println("[demo] Đã tạo " + cacTep.size() + " tệp thử, mỗi tệp 3 MiB");

        QuanLy ql = new QuanLy(host, port, tai.getPath(), kho.getPath());

        // Luồng in tiến trình định kỳ.
        boolean[] dangChay = {true};
        Thread baoCao = new Thread(() -> {
            // Mốc đo tốc độ theo từng LƯỢT (đối tượng LuotTruyen), không theo tên tệp: lượt tải tiếp/chạy lại
            // là đối tượng mới, mốc ban đầu = (offset bắt đầu, thời điểm bắt đầu) của chính lượt đó.
            Map<LuotTruyen, long[]> truoc = new java.util.IdentityHashMap<>();
            while (dangChay[0]) {
                try { Thread.sleep(800); } catch (InterruptedException e) { return; }
                List<LuotTruyen> ds;
                synchronized (ql) { ds = new ArrayList<>(ql.luot.values()); }
                for (LuotTruyen t : ds) {
                    if (t.trangThai.equals(CHAY) && t.tong > 0) {
                        long[] tr = truoc.getOrDefault(t, new long[] {t.batDauTu, t.t0});
                        long now = System.nanoTime();
                        long xong = t.xongByte;
                        double toc = Math.max(0, xong - tr[0]) / Math.max((now - tr[1]) / 1e9, 0.001);
                        truoc.put(t, new long[] {xong, now});
                        double pct = 100.0 * xong / t.tong;
                        System.out.println(String.format(Locale.ROOT, "[tiến trình] %s %s %5.1f%%  %s/s", t.ten, thanhTienDo(pct), pct,
                                Khung.dinhDangByte((long) toc)));
                    }
                }
            }
        });
        baoCao.setDaemon(true);
        baoCao.start();

        System.out.println("\n===== KỊCH BẢN: tải xuống 3 tệp đồng thời, giới hạn phiên 2 MiB/s =====");
        ql.datTocDoPhien(Khung.docSoByte("2M"));
        for (String ten : cacTep) ql.batDau(ten, "download", Khung.KHONG_GIOI_HAN);

        Thread.sleep(1200);
        ql.tamDung("tep2.bin");                            // tạm dừng giữa chừng
        Thread.sleep(800);
        ql.dungHan("tep3.bin");                            // dừng hẳn, xóa .part
        Thread.sleep(300);
        System.out.println("\n===== Nâng tốc độ phiên lên 8 MiB/s, tải tiếp tep2, chạy lại tep3 =====");
        ql.datTocDoPhien(Khung.docSoByte("8M"));
        ql.taiTiep("tep2.bin");                            // tải tiếp từ offset
        ql.batDau("tep3.bin", "download", Khung.KHONG_GIOI_HAN);   // chạy lại từ đầu

        choDen(() -> cacTep.stream().allMatch(t -> ql.luot.get(t).trangThai.equals(XONG) || ql.luot.get(t).trangThai.equals(LOI)), 30);
        ql.trangThai();

        System.out.println("\n===== KỊCH BẢN upload: đẩy tep1 (đã tải) lên server, giới hạn tệp 1 MiB/s =====");
        // Dùng tệp vừa tải về làm nguồn upload; đặt tên đích khác để không đè tệp gốc.
        QuanLy ql2 = new QuanLy(host, port, tai.getPath(), kho.getPath());
        ql2.session = ql.session;
        File dich = new File(nguon, "tep1_tai_len.bin");
        Files.copy(new File(tai, "tep1.bin").toPath(), dich.toPath());
        LuotTruyen t = new LuotTruyen("tep1_tai_len.bin", "upload", dich.getPath(), Khung.docSoByte("1M"));
        ql2.luot.put(t.ten, t);
        t.luong = new Thread(() -> ql2.taiLen(t));
        t.luong.setDaemon(true);
        t.t0 = System.nanoTime();
        t.luong.start();
        System.out.println("[client] BẮT ĐẦU upload tep1_tai_len.bin, giới hạn tệp 1.00 MiB/s");
        choDen(() -> t.trangThai.equals(XONG) || t.trangThai.equals(LOI), 20);

        dangChay[0] = false;
        Thread.sleep(200);
        System.out.println("\n===== TỔNG KẾT =====");
        for (String ten : cacTep) {
            LuotTruyen tt = ql.luot.get(ten);
            System.out.println("[demo] " + ten + ": " + tt.trangThai + ", " + Khung.dinhDangByte(tt.xongByte) + ", SHA-256 "
                    + (Boolean.TRUE.equals(tt.shaOk) ? "KHỚP" : "—"));
        }
        System.out.println("[demo] tep1_tai_len.bin: " + t.trangThai + ", server kiểm SHA-256 " + (Boolean.TRUE.equals(t.shaOk) ? "KHỚP" : "—"));
        String[] con = kho.list();
        List<String> ds = new ArrayList<>(List.of(con == null ? new String[0] : con));
        ds.sort(Comparator.naturalOrder());
        List<String> trich = new ArrayList<>();
        for (String d : ds) trich.add("'" + d + "'");
        System.out.println("[demo] Server còn giữ: [" + String.join(", ", trich) + "]");    // in như danh sách Python

        srv.close();
        xoaCay(goc);
        System.out.println("[demo] Đã dọn thư mục tạm " + goc);
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 3) {
            System.err.println("Cách dùng: java -cp out TruyenClient client|demo <host> <port> [thư_mục_tải]");
            System.exit(1);
        }
        String vai = args[0], host = args[1];
        int port = Integer.parseInt(args[2]);
        switch (vai) {
            case "client" -> chayGiaoDien(host, port, args.length > 3 ? args[3] : "tai_ve");
            case "demo" -> chayDemo(host, port);
            default -> { System.err.println("Vai phải là client hoặc demo"); System.exit(1); }
        }
        System.exit(0);
    }
}
