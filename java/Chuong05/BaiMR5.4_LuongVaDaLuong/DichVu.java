import java.io.BufferedReader;
import java.io.IOException;
import java.io.PrintWriter;
import java.net.InetAddress;
import java.net.ServerSocket;
import java.net.Socket;
import java.nio.charset.StandardCharsets;

/**
 * Bài mở rộng MR5.4 – Dịch vụ thử (máy chủ phụ trợ) cho các vai trò luong, hang-doi.
 * Giao thức giống bản Python/Go, mỗi kết nối một lệnh, mỗi kết nối một luồng:
 *   VIEC &lt;mã&gt; &lt;ms&gt;  → chờ &lt;ms&gt; rồi trả "XONG &lt;mã&gt;"
 *   PHAT &lt;n&gt;        → gửi n dòng "GOI k dữ-liệu", đọc n dòng "KQ k tổng-byte", trả "NHAN đúng/n"
 *   THONGKE         → trả "TK số-yêu-cầu-VIEC đồng-thời-tối-đa", in tóm tắt "đợt", đặt lại
 */
final class DichVu {
    // Dữ liệu của gói thứ k (giống nhau ở mọi bản ngôn ngữ)
    static String duLieu(int k) { return ("goi-so-" + k + "-").repeat(3); }

    // Kết quả xử lý một gói: tổng giá trị các byte UTF-8, lấy dư 65536
    static int tongByte(String s) {
        int t = 0;
        for (byte b : s.getBytes(StandardCharsets.UTF_8)) t += b & 0xFF;
        return t % 65536;
    }

    static final Object KHOA = new Object();
    static int tong, dangChay, toiDa;

    static void phucVu(Socket s) {
        try (s) {
            BufferedReader r = NhatKy.doc(s);
            PrintWriter w = NhatKy.ghi(s);
            String line = r.readLine();
            if (line == null) return;
            String[] p = line.trim().split(" ");
            switch (p[0]) {
                case "VIEC" -> {
                    synchronized (KHOA) { tong++; dangChay++; toiDa = Math.max(toiDa, dangChay); }
                    Thread.sleep(Integer.parseInt(p[2]));
                    synchronized (KHOA) { dangChay--; }
                    w.println("XONG " + p[1]);
                }
                case "PHAT" -> {
                    int n = Integer.parseInt(p[1]);
                    StringBuilder sb = new StringBuilder();
                    for (int k = 1; k <= n; k++) sb.append("GOI ").append(k).append(' ').append(duLieu(k)).append('\n');
                    w.print(sb);
                    w.flush();
                    int dung = 0;
                    for (int i = 0; i < n; i++) {
                        String[] kq = r.readLine().split(" ");
                        if (kq.length == 3 && Integer.parseInt(kq[2]) == tongByte(duLieu(Integer.parseInt(kq[1])))) dung++;
                    }
                    w.println("NHAN " + dung + "/" + n);
                    NhatKy.say(NhatKy.f("[dịch vụ] PHAT %d: nhận %d kết quả, đúng %d/%d", n, n, dung, n));
                }
                case "THONGKE" -> {
                    int a, b;
                    synchronized (KHOA) { a = tong; b = toiDa; tong = toiDa = 0; }
                    w.println("TK " + a + " " + b);
                    NhatKy.say(NhatKy.f("[dịch vụ] đợt: %d yêu cầu VIEC, đồng thời tối đa %d", a, b));
                }
                default -> { }
            }
        } catch (IOException | InterruptedException e) {
            NhatKy.say("[dịch vụ] lỗi: " + e.getMessage());
        }
    }

    static void run(String host, int port) throws IOException {
        try (ServerSocket ss = new ServerSocket(port, 256, InetAddress.getByName(host))) {
            NhatKy.say(NhatKy.f("[dịch vụ] lắng nghe %s:%d (mỗi kết nối một luồng)", host, port));
            while (true) {
                Socket s = ss.accept();
                Thread t = new Thread(() -> phucVu(s));
                t.setDaemon(true);
                t.start();
            }
        }
    }
}
