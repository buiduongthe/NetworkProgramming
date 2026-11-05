import java.io.ByteArrayOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.net.InetAddress;
import java.net.InetSocketAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.net.UnknownHostException;
import java.nio.charset.StandardCharsets;
import java.util.ArrayList;
import java.util.HashMap;
import java.util.List;
import java.util.Map;

/**
 * Bài mở rộng MR8.3 – Phòng thủ tấn công từ chối dịch vụ mức ứng dụng kiểu Slowloris.
 *
 * Bài mở rộng, KHÔNG có trong giáo trình; nối tiếp Chương 6 (server nhiều client, Bài 6.1/6.2) và Chương 8
 * (vận hành an toàn). Mục đích DUY NHẤT là dạy cách *phòng thủ*: so sánh một server HTTP/1.1 nhỏ ở hai chế độ
 * và đo xem một yêu cầu bình thường còn được phục vụ hay không.
 *
 *   server vulnerable <host> <port>   – nhóm luồng cố định, KHÔNG đặt hạn đọc tiêu đề (dễ bị đánh gục)
 *   server protected  <host> <port>   – có hạn đọc tiêu đề, cỡ tiêu đề tối đa, hạn chót rỗi,
 *                                        giới hạn kết nối theo IP và trần kết nối toàn cục (trả 503)
 *   slow   <host> <port> [N] [nguồn]  – CLIENT THÍ NGHIỆM: mở N kết nối tới 127.0.0.1 và nhỏ giọt từng byte
 *                                        tiêu đề (chỉ cho loopback – từ chối mọi đích khác). Mặc định dùng địa
 *                                        chỉ nguồn 127.0.0.2 để tách IP "kẻ tấn công" khỏi IP của probe
 *                                        (127.0.0.1), nhờ đó thấy rõ giới hạn theo IP.
 *   probe  <host> <port>              – gửi MỘT yêu cầu HTTP bình thường, đo thành công/độ trễ
 *
 * CẢNH BÁO ĐẠO ĐỨC: vai "slow" chỉ dùng trong phòng thí nghiệm cô lập, chỉ nhắm 127.0.0.1 của chính máy bạn,
 * để kiểm chứng biện pháp phòng thủ. Tuyệt đối không nhắm máy/dịch vụ của người khác.
 *
 * Chạy: java -cp out ChongSlowloris server vulnerable|protected <host> <port> | slow <host> <port> [N] | probe <host> <port>
 */
public class ChongSlowloris {
    // ------------------------------- Tham số phòng thủ (chế độ protected) -------------------------------
    static final double HAN_DOC_TIEU_DE = 3.0;    // giây: toàn bộ phần đầu (request head) phải xong trong hạn này
    static final double HAN_CHOT_ROI = 2.0;       // giây: không nhận thêm byte nào trong khoảng này -> coi là rỗi, đóng
    static final int CO_TIEU_DE_TOI_DA = 8 * 1024;   // byte: tổng kích thước phần đầu tối đa
    static final int GIOI_HAN_IP = 10;            // số kết nối đồng thời tối đa cho mỗi địa chỉ IP
    static final int TRAN_TOAN_CUC = 64;          // số kết nối đồng thời tối đa toàn server (vượt -> trả 503 rồi đóng)
    static final int SO_LUONG_WORKER = 40;        // nhóm luồng cố định (giống MaxRequestWorkers của Apache prefork)
    static final byte[] TRA_LOI = ("HTTP/1.1 200 OK\r\nContent-Type: text/plain; charset=utf-8\r\n"
            + "Connection: close\r\nContent-Length: 20\r\n\r\nXin chao tu server!\n").getBytes(StandardCharsets.US_ASCII);
    static final byte[] CRLFCRLF = {'\r', '\n', '\r', '\n'};

    /** Kiểm tra host có phải địa chỉ loopback (127.0.0.0/8, ::1) hay không – dùng cho vai 'slow'. */
    static boolean loopbackHayKhong(String host) {
        try {
            for (InetAddress a : InetAddress.getAllByName(host)) if (!a.isLoopbackAddress()) return false;
            return true;
        } catch (UnknownHostException e) {
            return false;
        }
    }

    static boolean coCrlfCrlf(byte[] b, int n) {
        for (int i = 0; i + 3 < n; i++)
            if (b[i] == '\r' && b[i + 1] == '\n' && b[i + 2] == '\r' && b[i + 3] == '\n') return true;
        return false;
    }

    // =================================== SERVER ===================================
    /** Bộ đếm có khóa để các luồng worker cùng ghi. */
    static final class ThongKe {
        int chapNhan, xong, dongCham, tuChoi503;
        final Map<String, Integer> demIp = new HashMap<>();   // IP -> số kết nối đang mở (chỉ chế độ protected)
    }

    /**
     * Chế độ PROTECTED: đọc phần đầu HTTP với ba lớp chặn Slowloris:
     * (1) hạn chót tuyệt đối HAN_DOC_TIEU_DE cho toàn bộ phần đầu; (2) hạn chót rỗi HAN_CHOT_ROI giữa hai lần nhận
     * byte (nhỏ giọt -> timeout); (3) tổng cỡ tiêu đề không vượt CO_TIEU_DE_TOI_DA.
     * Trả về true nếu nhận đủ phần đầu (kết thúc bằng CRLFCRLF), false nếu phải đóng sớm.
     */
    static boolean docTieuDeCoHan(Socket conn) throws IOException {
        ByteArrayOutputStream duLieu = new ByteArrayOutputStream();
        long hanTuyetDoi = System.nanoTime() + (long) (HAN_DOC_TIEU_DE * 1e9);
        InputStream in = conn.getInputStream();
        byte[] buf = new byte[1024];
        while (!coCrlfCrlf(duLieu.toByteArray(), duLieu.size())) {
            long conLaiMs = (hanTuyetDoi - System.nanoTime()) / 1_000_000;
            if (conLaiMs <= 0) return false;                          // (1) quá hạn tổng
            conn.setSoTimeout((int) Math.min(HAN_CHOT_ROI * 1000, conLaiMs));   // (2) hạn chót rỗi
            int n;
            try {
                n = in.read(buf);
            } catch (SocketTimeoutException e) {
                return false;
            }
            if (n == -1) return false;                                // client đóng giữa chừng
            duLieu.write(buf, 0, n);
            if (duLieu.size() > CO_TIEU_DE_TOI_DA) return false;      // (3) tiêu đề quá to
        }
        return true;
    }

    /** Chế độ VULNERABLE: đọc phần đầu HTTP, KHÔNG đặt hạn -> một client nhỏ giọt giữ worker mãi. */
    static boolean docTieuDeKhongHan(Socket conn) throws IOException {
        conn.setSoTimeout(0);                                         // 0 = chờ mãi
        ByteArrayOutputStream duLieu = new ByteArrayOutputStream();
        InputStream in = conn.getInputStream();
        byte[] buf = new byte[1024];
        while (!coCrlfCrlf(duLieu.toByteArray(), duLieu.size())) {
            int n = in.read(buf);
            if (n == -1) return false;
            duLieu.write(buf, 0, n);
        }
        return true;
    }

    /** Một luồng trong nhóm: lặp lấy kết nối từ accept và phục vụ. */
    static void worker(ServerSocket sock, String mode, ThongKe stat) {
        while (true) {
            Socket conn;
            try {
                conn = sock.accept();
            } catch (IOException e) {
                return;
            }
            String ip = conn.getInetAddress().getHostAddress();
            synchronized (stat) { stat.chapNhan++; }
            if (mode.equals("protected")) {
                boolean quaTai;
                synchronized (stat) {                                 // trần toàn cục và giới hạn theo IP
                    int tong = stat.demIp.values().stream().mapToInt(Integer::intValue).sum();
                    int rieng = stat.demIp.getOrDefault(ip, 0);
                    quaTai = tong >= TRAN_TOAN_CUC || rieng >= GIOI_HAN_IP;
                    if (!quaTai) stat.demIp.put(ip, rieng + 1);
                }
                if (quaTai) {                                         // vượt -> 503 rồi đóng ngay (giải phóng worker)
                    synchronized (stat) { stat.tuChoi503++; }
                    try (conn) {
                        conn.getOutputStream().write("HTTP/1.1 503 Service Unavailable\r\nConnection: close\r\nContent-Length: 0\r\n\r\n"
                                .getBytes(StandardCharsets.US_ASCII));
                    } catch (IOException ignored) { }
                    continue;
                }
                try (conn) {
                    if (docTieuDeCoHan(conn)) {
                        conn.getOutputStream().write(TRA_LOI);
                        synchronized (stat) { stat.xong++; }
                    } else {
                        synchronized (stat) { stat.dongCham++; }      // đóng kết nối gửi tiêu đề quá chậm
                    }
                } catch (IOException ignored) {
                } finally {
                    synchronized (stat) {
                        int v = stat.demIp.getOrDefault(ip, 1) - 1;
                        if (v <= 0) stat.demIp.remove(ip);
                        else stat.demIp.put(ip, v);
                    }
                }
            } else {                                                  // vulnerable
                try (conn) {
                    if (docTieuDeKhongHan(conn)) {                    // KẸT Ở ĐÂY nếu client nhỏ giọt
                        conn.getOutputStream().write(TRA_LOI);
                        synchronized (stat) { stat.xong++; }
                    }
                } catch (IOException ignored) { }
            }
        }
    }

    static void chayServer(String mode, String host, int port) throws Exception {
        ServerSocket sock = new ServerSocket();
        sock.setReuseAddress(true);
        sock.bind(new InetSocketAddress(host, port), 256);
        ThongKe stat = new ThongKe();
        System.out.println("[server:" + mode + "] Nghe tại " + host + ":" + port + ", nhóm " + SO_LUONG_WORKER + " worker. Ctrl+C để dừng.");
        if (mode.equals("protected"))
            System.out.println("[server:protected] Phòng thủ: hạn đọc tiêu đề " + HAN_DOC_TIEU_DE + "s, hạn rỗi " + HAN_CHOT_ROI
                    + "s, tiêu đề <= " + CO_TIEU_DE_TOI_DA + "B, <= " + GIOI_HAN_IP + " kết nối/IP, trần " + TRAN_TOAN_CUC + " kết nối.");
        else
            System.out.println("[server:vulnerable] KHÔNG có hạn đọc tiêu đề: mỗi kết nối chậm giữ một worker.");
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {       // Ctrl+C: in tổng kết
            synchronized (stat) {
                System.out.println("[server:" + mode + "] Dừng. Tổng kết: chấp nhận " + stat.chapNhan + ", phục vụ xong " + stat.xong
                        + ", đóng vì chậm " + stat.dongCham + ", từ chối 503 " + stat.tuChoi503 + ".");
            }
        }));
        List<Thread> threads = new ArrayList<>();
        for (int i = 0; i < SO_LUONG_WORKER; i++) threads.add(Thread.ofPlatform().daemon().start(() -> worker(sock, mode, stat)));
        while (true) Thread.sleep(500);
    }

    // =================================== CLIENT THÍ NGHIỆM (chỉ loopback) ===================================
    /**
     * Mở n kết nối và nhỏ giọt từng byte tiêu đề, KHÔNG bao giờ kết thúc phần đầu. Chỉ chạy được khi host là
     * loopback của chính máy bạn. Giữ kết nối khoảng 9 giây rồi thoát. Dùng địa chỉ nguồn loopback riêng
     * (mặc định 127.0.0.2) để server phân biệt với probe.
     */
    static void chaySlow(String host, int port, int n, String nguon) throws Exception {
        if (!loopbackHayKhong(host)) {
            System.err.println("[slow] TỪ CHỐI: đích " + host + " không phải loopback. Vai này chỉ dùng cho 127.0.0.1 trong phòng thí nghiệm của chính bạn.");
            System.exit(1);
        }
        if (!loopbackHayKhong(nguon) || !nguon.matches("[0-9.:]+")) {
            System.err.println("[slow] TỪ CHỐI: địa chỉ nguồn " + nguon + " không phải loopback.");
            System.exit(1);
        }
        System.out.println("[slow] (CHỈ THÍ NGHIỆM) mở " + n + " kết nối nhỏ giọt tới " + host + ":" + port + " từ nguồn " + nguon + " trong ~9s");
        List<Socket> socks = new ArrayList<>();
        Runtime.getRuntime().addShutdownHook(new Thread(() -> {       // Ctrl+C hoặc hết giờ: đóng hết
            synchronized (socks) {
                for (Socket s : socks) try { s.close(); } catch (IOException ignored) { }
            }
            System.out.println("[slow] đóng toàn bộ kết nối thí nghiệm");
        }));
        for (int i = 0; i < n; i++) {
            try {
                Socket s = new Socket();
                s.bind(new InetSocketAddress(nguon, 0));             // chọn địa chỉ nguồn trước khi CONNECT
                s.connect(new InetSocketAddress(host, port), 2000);
                s.getOutputStream().write("GET /?slow HTTP/1.1\r\nHost: localhost\r\n".getBytes(StandardCharsets.US_ASCII));   // phần đầu CHƯA kết thúc
                synchronized (socks) { socks.add(s); }
            } catch (IOException ignored) { }
        }
        System.out.println("[slow] đã mở được " + socks.size() + " kết nối; bắt đầu nhỏ giọt mỗi ~7s một byte");
        long hetHan = System.nanoTime() + 9_000_000_000L;
        while (System.nanoTime() < hetHan) {
            Thread.sleep(7000);
            int song = 0;
            synchronized (socks) {
                for (Socket s : new ArrayList<>(socks)) {
                    try {
                        s.getOutputStream().write('X');               // một byte rác, giữ kết nối "còn sống"
                        song++;
                    } catch (IOException e) {
                        socks.remove(s);
                    }
                }
            }
            System.out.println("[slow] còn " + song + " kết nối đang giữ");
        }
        System.exit(0);
    }

    // =================================== PROBE (yêu cầu bình thường) ===================================
    /** Gửi một yêu cầu HTTP/1.1 bình thường, đo xem có được phục vụ không (hạn 4 s). */
    static void chayProbe(String host, int port) {
        long t0 = System.nanoTime();
        try (Socket s = new Socket()) {
            s.connect(new InetSocketAddress(host, port), 4000);
            s.setSoTimeout(4000);
            s.getOutputStream().write("GET / HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n".getBytes(StandardCharsets.US_ASCII));
            ByteArrayOutputStream data = new ByteArrayOutputStream();
            InputStream in = s.getInputStream();
            byte[] buf = new byte[1024];
            while (!coCrlfCrlf(data.toByteArray(), data.size())) {
                int n = in.read(buf);
                if (n == -1) break;
                data.write(buf, 0, n);
            }
            long dt = (System.nanoTime() - t0) / 1_000_000;
            String text = data.toString(StandardCharsets.ISO_8859_1);
            String dongDau = text.isEmpty() ? "(rỗng)" : text.split("\r\n", 2)[0];
            if (text.startsWith("HTTP/1.1 200")) System.out.println("[probe] THÀNH CÔNG sau " + dt + " ms: " + dongDau);
            else System.out.println("[probe] phản hồi khác: " + dongDau + " (" + dt + " ms)");
        } catch (SocketTimeoutException e) {
            long dt = (System.nanoTime() - t0) / 1_000_000;
            System.out.println("[probe] THẤT BẠI: hết thời gian chờ sau " + dt + " ms – server không phục vụ kịp (bị tấn công làm nghẽn).");
        } catch (IOException e) {
            System.out.println("[probe] THẤT BẠI: " + e.getMessage());
        }
    }

    public static void main(String[] a) throws Exception {
        if (a.length >= 4 && a[0].equals("server") && (a[1].equals("vulnerable") || a[1].equals("protected"))) chayServer(a[1], a[2], Integer.parseInt(a[3]));
        else if (a.length >= 3 && a[0].equals("slow")) chaySlow(a[1], Integer.parseInt(a[2]), a.length > 3 ? Integer.parseInt(a[3]) : 150, a.length > 4 ? a[4] : "127.0.0.2");
        else if (a.length >= 3 && a[0].equals("probe")) chayProbe(a[1], Integer.parseInt(a[2]));
        else {
            System.out.println("Cách dùng:");
            System.out.println("  java -cp out ChongSlowloris server vulnerable|protected <host> <port>");
            System.out.println("  java -cp out ChongSlowloris slow  <host> <port> [N]   (chỉ 127.0.0.1)");
            System.out.println("  java -cp out ChongSlowloris probe <host> <port>");
            System.exit(1);
        }
    }
}
