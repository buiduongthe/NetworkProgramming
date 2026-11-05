import java.io.BufferedReader;
import java.io.File;
import java.io.IOException;
import java.io.InputStreamReader;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.nio.file.Files;
import java.util.ArrayList;
import java.util.Arrays;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.Random;
import java.util.concurrent.locks.LockSupport;
import javax.sound.sampled.AudioSystem;
import javax.sound.sampled.SourceDataLine;
import javax.sound.sampled.TargetDataLine;

/**
 * Bài mở rộng MR6.2 – Client phòng hội thoại âm thanh.
 *
 * Mỗi 20 ms: (1) thu một khung từ nguồn (sin tổng hợp – mỗi client một tần số riêng để phân tích được ai
 * đang nói; hoặc tệp WAV; hoặc micro thật với --mic), đóng gói RTP, gửi UDP nếu mic mở; (2) lấy một khung
 * từ bộ đệm jitter, nhân âm lượng nghe của client, ghi vào bản ghi (--ghi tệp.wav), phát ra loa nếu có --loa.
 * Micro, loa dùng javax.sound.sampled (có sẵn trong JDK); máy không có thiết bị âm thanh thì tự dùng nguồn tổng hợp.
 *
 * Chạy: java -cp out KhachAmThanh <tên> <host> <cổng_tcp> [--tan-so 440] [--bien-do 0.25] [--wav tệp.wav]
 *           [--mic] [--loa] [--ghi out/ghi.wav] [--thoi-gian 10] [--mat-goi 0.05] [--dem 3]
 * Lệnh (stdin): tat_mic, bat_mic, am_luong <hệ số>, thoat; "@t lệnh" = chờ tới giây thứ t.
 */
public class KhachAmThanh {
    /** Nơi lấy 320 mẫu mỗi 20 ms. */
    interface NguonAm { short[] layKhung(); }

    /** Sin tần số f, pha liên tục giữa các khung. */
    static final class NguonSin implements NguonAm {
        final double f, a;
        int n;
        NguonSin(double f, double a) { this.f = f; this.a = a; }
        public short[] layKhung() {
            double w = 2 * Math.PI * f / AmThanh.TAN_SO_MAU;
            short[] k = new short[AmThanh.MAU_MOI_KHUNG];
            for (int i = 0; i < k.length; i++) k[i] = (short) Math.round(a * Math.sin(w * (n + i)));
            n += AmThanh.MAU_MOI_KHUNG;
            return k;
        }
    }

    /** Phát lặp tệp WAV. */
    static final class NguonWAV implements NguonAm {
        final short[] mau;
        int vt;
        NguonWAV(short[] mau) { this.mau = mau; }
        public short[] layKhung() {
            short[] k = new short[AmThanh.MAU_MOI_KHUNG];
            for (int i = 0; i < k.length; i++) k[i] = mau[(vt + i) % mau.length];
            vt += AmThanh.MAU_MOI_KHUNG;
            return k;
        }
    }

    /** Micro thật: đọc 640 byte (một khung) từ TargetDataLine (định dạng 16 kHz, 16 bit, mono, big-endian). */
    static final class NguonMic implements NguonAm {
        final TargetDataLine line;
        NguonMic(TargetDataLine line) { this.line = line; }
        public short[] layKhung() {
            byte[] b = new byte[AmThanh.BYTE_MOI_KHUNG];
            int n = line.read(b, 0, b.length);                     // chặn tới khi đủ một khung
            return AmThanh.byteSangPCM(n == b.length ? b : Arrays.copyOf(b, b.length));
        }
    }

    static final Object khoa = new Object();
    static boolean micMo = false, dung = false;
    static double amLuong = 1.0;
    static short[] banGhi = new short[16000];
    static int soMau = 0;
    static final List<Object> dsMoc = new ArrayList<>();

    static void ghiThem(short[] mau) {                             // nối vào bản ghi (mảng tự giãn)
        if (soMau + mau.length > banGhi.length) banGhi = Arrays.copyOf(banGhi, banGhi.length * 2);
        System.arraycopy(mau, 0, banGhi, soMau, mau.length);
        soMau += mau.length;
    }

    static void danhMoc(String text) {
        synchronized (khoa) {
            dsMoc.add(Json.obj("t", Math.round((double) soMau / AmThanh.TAN_SO_MAU * 1000) / 1000.0, "su_kien", text));
        }
    }

    @SuppressWarnings("unchecked")
    public static void main(String[] args) throws Exception {
        String ten = args[0], host = args[1];
        int cong = Integer.parseInt(args[2]);
        double tanSo = 440, bienDo = 0.25, thoiGian = 10, matGoi = 0;
        String wav = "", ghi = "";
        int dem = 3;
        boolean mic = false, loa = false;
        for (int i = 3; i < args.length; i++) {
            switch (args[i]) {
                case "--tan-so" -> tanSo = Double.parseDouble(args[++i]);
                case "--bien-do" -> bienDo = Double.parseDouble(args[++i]);
                case "--wav" -> wav = args[++i];
                case "--ghi" -> ghi = args[++i];
                case "--thoi-gian" -> thoiGian = Double.parseDouble(args[++i]);
                case "--mat-goi" -> matGoi = Double.parseDouble(args[++i]);
                case "--dem" -> dem = Integer.parseInt(args[++i]);
                case "--mic" -> mic = true;
                case "--loa" -> loa = true;
                default -> { System.err.println("Tham số lạ: " + args[i]); System.exit(2); }
            }
        }
        String tag = "[khach " + ten + "]";
        java.util.function.Consumer<String> log = s -> System.out.println(tag + " " + s);

        NguonAm nguon = new NguonSin(tanSo, bienDo * 32767);
        String moTa = "sin " + fmtG(tanSo) + " Hz, biên độ " + fmtG(bienDo);
        if (!wav.isEmpty()) {
            try {
                nguon = new NguonWAV(AmThanh.docWAV(new File(wav)));
                moTa = "tệp " + wav;
            } catch (IOException e) {
                log.accept("Lỗi đọc " + wav + ": " + e.getMessage());
                System.exit(1);
            }
        }
        SourceDataLine loaRa = null;
        if (mic || loa) {                                          // thiết bị âm thanh thật (tùy chọn)
            try {
                if (mic) {
                    TargetDataLine t = AudioSystem.getTargetDataLine(AmThanh.DINH_DANG);
                    t.open(AmThanh.DINH_DANG, AmThanh.BYTE_MOI_KHUNG * 4);
                    t.start();
                    nguon = new NguonMic(t);
                    moTa = "micro thật";
                }
                if (loa) {
                    loaRa = AudioSystem.getSourceDataLine(AmThanh.DINH_DANG);
                    loaRa.open(AmThanh.DINH_DANG, AmThanh.BYTE_MOI_KHUNG * 8);
                    loaRa.start();
                }
            } catch (Exception e) {                                // LineUnavailableException, IllegalArgumentException…
                log.accept("Không dùng được thiết bị âm thanh (" + e.getMessage() + "); dùng nguồn tổng hợp/WAV, không phát loa");
                loaRa = null;
            }
        }

        // ---- Kênh điều khiển TCP: vào phòng
        Socket tcp = new Socket();
        try {
            tcp.connect(new InetSocketAddress(host, cong), 5000);
        } catch (IOException e) {
            log.accept("Không kết nối được: " + e.getMessage());
            System.exit(1);
        }
        tcp.setTcpNoDelay(true);
        BufferedReader sc = new BufferedReader(new InputStreamReader(tcp.getInputStream(), StandardCharsets.UTF_8));
        AmThanh.guiJSON(tcp, Json.obj("loai", "vao", "ten", ten));
        Map<String, Object> chao = ServerAmThanh.docJSON(String.valueOf(sc.readLine()));
        if (chao == null || !(chao.get("ma") instanceof Number)) {
            log.accept("Server không chào");
            System.exit(1);
        }
        int ma = ((Number) chao.get("ma")).intValue(), congUDP = ((Number) chao.get("cong_udp")).intValue();
        log.accept("Vào phòng, mã " + ma + "; nguồn: " + moTa);
        log.accept("Gửi " + AmThanh.MAU_MOI_KHUNG + " mẫu/" + AmThanh.KHUNG_MS + " ms -> 50 gói/s, 256 kbit/s PCM");

        // ---- Kênh dữ liệu UDP: connect() để chỉ nhận gói từ đúng địa chỉ server
        DatagramSocket udp = new DatagramSocket();
        InetSocketAddress srvUdp = new InetSocketAddress(host, congUDP);
        udp.connect(srvUdp);
        AmThanh.BoDemJitter boDem = new AmThanh.BoDemJitter(dem);

        Thread.ofPlatform().daemon().start(() -> {                 // đọc kênh điều khiển
            try {
                String line;
                while ((line = sc.readLine()) != null) {
                    Map<String, Object> tin = ServerAmThanh.docJSON(line);
                    if (tin == null) break;
                    Object loai = tin.get("loai");
                    if ("mic".equals(loai)) {
                        synchronized (khoa) { micMo = Boolean.TRUE.equals(tin.get("mo")); }
                        log.accept("Mic: " + tin.get("ly_do"));
                    } else if ("su_kien".equals(loai)) {
                        log.accept("Sự kiện: " + tin.get("noi_dung"));
                        danhMoc(String.valueOf(tin.get("noi_dung")));
                    } else if ("tu_choi".equals(loai)) {
                        log.accept("Bị từ chối: " + tin.get("ly_do"));
                    }
                }
            } catch (IOException ignored) { }
            synchronized (khoa) { dung = true; }                   // server đóng kết nối -> client dừng
        });

        Thread.ofPlatform().daemon().start(() -> {                 // nhận luồng trộn từ server
            byte[] buf = new byte[2048];
            while (true) {
                DatagramPacket p = new DatagramPacket(buf, buf.length);
                try {
                    udp.receive(p);
                } catch (IOException e) {
                    return;
                }
                AmThanh.GoiRTP g = AmThanh.tachRTP(p.getData(), p.getLength());
                if (g != null && g.pt() == AmThanh.PT_AM_THANH)
                    synchronized (khoa) { boDem.nhanGoi(g.seq(), g.ts(), g.payload(), AmThanh.giay()); }
            }
        });

        Thread.ofPlatform().daemon().start(() -> AmThanh.chayKichBan(dong -> {   // lệnh bàn phím / kịch bản
            String[] p = dong.split("\\s+");
            if (p[0].equals("tat_mic")) {                          // tự tắt: có hiệu lực ngay, không cần server đồng ý
                synchronized (khoa) { micMo = false; }
                log.accept("> tat_mic");
                AmThanh.guiJSON(tcp, Json.obj("loai", "tu_tat_mic"));
            } else if (p[0].equals("bat_mic")) {                   // tự bật: phải chờ server cho phép
                log.accept("> bat_mic (xin server)");
                AmThanh.guiJSON(tcp, Json.obj("loai", "tu_bat_mic"));
            } else if (p[0].equals("am_luong") && p.length == 2) {
                double v = Double.parseDouble(p[1]);
                synchronized (khoa) { amLuong = v; }
                log.accept(String.format(Locale.ROOT, "> am_luong %.2f (chỉ áp dụng tại client)", v));
                danhMoc(String.format(Locale.ROOT, "%s tự đặt âm lượng nghe %.2f", ten, v));
                AmThanh.guiJSON(tcp, Json.obj("loai", "am_luong_nghe", "gia_tri", v));
            } else if (p[0].equals("thoat")) {
                synchronized (khoa) { dung = true; }
            } else {
                log.accept("lệnh không hợp lệ: " + dong);
            }
            return true;
        }));

        // ---- Vòng lặp 20 ms
        Random rng = new Random(ma);                               // giả lập mất gói tất định theo mã client
        int seq = 0;
        long ts = 0;
        int daGui = 0, boGiaLap = 0;
        long tongByte = 0;
        int soKhung = (int) (thoiGian * 1000 / AmThanh.KHUNG_MS);
        byte[] giu = AmThanh.dongGoiRTP(AmThanh.PT_GIU_KET_NOI, 0, 0, ma, new byte[0]);
        udp.send(new DatagramPacket(giu, giu.length));             // đăng ký địa chỉ UDP với server ngay từ đầu
        short[] cuaSo = new short[50 * AmThanh.MAU_MOI_KHUNG];
        int nCuaSo = 0;
        boolean[] cat = new boolean[1];
        long hen = System.nanoTime();
        for (int n = 1; n <= soKhung; n++) {
            boolean mo;
            double al;
            synchronized (khoa) {
                if (dung) break;
                mo = micMo;
                al = amLuong;
            }
            // 1. Thu và gửi
            short[] khung = nguon.layKhung();
            if (mo) {
                if (rng.nextDouble() < matGoi) {
                    boGiaLap++;                                    // "mạng" làm rơi gói: số thứ tự vẫn tăng
                } else {
                    byte[] goi = AmThanh.dongGoiRTP(AmThanh.PT_AM_THANH, seq, ts, ma, AmThanh.pcmSangByte(khung));
                    udp.send(new DatagramPacket(goi, goi.length));
                    daGui++;
                    tongByte += goi.length + 28;                   // + header UDP (8) và IPv4 (20)
                }
                seq = (seq + 1) & 0xFFFF;
            } else if (n % 50 == 0) {
                udp.send(new DatagramPacket(giu, giu.length));     // mic tắt: mỗi giây một gói giữ kết nối
            }
            ts = (ts + AmThanh.MAU_MOI_KHUNG) & 0xFFFFFFFFL;       // nhãn thời gian tăng theo thời gian thực, kể cả khi tắt mic
            // 2. Nhận và phát
            byte[] p;
            synchronized (khoa) { p = boDem.lay(); }
            short[] mau = p != null ? AmThanh.byteSangPCM(p) : new short[AmThanh.MAU_MOI_KHUNG];
            if (al != 1) for (int i = 0; i < mau.length; i++) mau[i] = AmThanh.catNgon(mau[i] * al, cat);
            synchronized (khoa) { ghiThem(mau); }
            if (loaRa != null) {
                byte[] b = AmThanh.pcmSangByte(mau);
                loaRa.write(b, 0, b.length);
            }
            System.arraycopy(mau, 0, cuaSo, nCuaSo, mau.length);
            nCuaSo += mau.length;
            if (n % 50 == 0) {
                double db = AmThanh.mucDBFS(cuaSo, nCuaSo);
                log.accept(String.format(Locale.ROOT, "%4.1fs nghe %s %s dBFS mic %s", n * AmThanh.KHUNG_MS / 1000.0,
                        AmThanh.thanhMuc(db, 16), AmThanh.chuDB(db, 5), mo ? "mở" : "tắt"));
                nCuaSo = 0;
            }
            hen += AmThanh.KHUNG_MS * 1_000_000L;                  // lịch cố định: không trôi theo thời gian xử lý
            long cho = hen - System.nanoTime();
            if (cho > 0) LockSupport.parkNanos(cho);
        }

        // ---- Kết thúc: thống kê, lưu bản ghi
        double giay = AmThanh.giay();
        tcp.close();
        udp.close();
        log.accept(String.format(Locale.ROOT, "Kết thúc sau %.1f s", giay));
        log.accept(String.format(Locale.ROOT, "Gửi %d gói (%.0f kbit/s gồm header), giả lập mất %d", daGui, tongByte * 8 / giay / 1000, boGiaLap));
        synchronized (khoa) {
            AmThanh.BoDemJitter b = boDem;
            log.accept(String.format(Locale.ROOT, "Nhận %d gói, mất %d, trễ %d, jitter %.2f ms", b.nhan, b.mat(), b.tre, b.jitterMs()));
            log.accept("Phát: thiếu " + b.thieu + " khung, cạn " + b.can + " lần, đệm " + dem + " khung = " + dem * AmThanh.KHUNG_MS + " ms");
            if (!ghi.isEmpty()) {
                File f = new File(ghi);
                if (f.getParentFile() != null) f.getParentFile().mkdirs();
                AmThanh.ghiWAV(f, banGhi, soMau);
                String duongMoc = ghi.replaceAll("\\.[^./]*$", "") + "_moc.json";
                Files.writeString(new File(duongMoc).toPath(), Json.stringify(Json.obj("ten", ten, "moc", dsMoc)) + "\n", StandardCharsets.UTF_8);
                log.accept(String.format(Locale.ROOT, "Đã ghi %s (%.2f s), %d mốc sự kiện", ghi, (double) soMau / AmThanh.TAN_SO_MAU, dsMoc.size()));
            }
        }
        if (loaRa != null) loaRa.drain();
        System.exit(0);
    }

    /** In số như %g của Go/Python: 440 -> "440", 0.25 -> "0.25". */
    static String fmtG(double v) {
        return v == Math.rint(v) && Math.abs(v) < 1e15 ? String.valueOf((long) v) : String.valueOf(v);
    }
}
