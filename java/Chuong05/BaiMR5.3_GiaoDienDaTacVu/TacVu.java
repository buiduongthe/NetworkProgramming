import java.io.IOException;
import java.io.InputStream;
import java.net.DatagramPacket;
import java.net.DatagramSocket;
import java.net.HttpURLConnection;
import java.net.InetSocketAddress;
import java.net.Socket;
import java.net.SocketTimeoutException;
import java.net.URI;
import java.nio.charset.StandardCharsets;
import java.util.List;
import java.util.concurrent.atomic.AtomicBoolean;

/**
 * Bài mở rộng MR5.3 – Các tác vụ mạng chạy nền và bộ giới hạn đồng thời (không dùng Swing).
 *
 * Mọi hàm ở đây chạy trên luồng nền, KHÔNG chạm vào thành phần Swing. Tiến trình được báo qua
 * progress.report(phần_trăm, chữ) do giao diện cung cấp (giao diện chuyển nó về luồng giao diện).
 * Hủy theo kiểu hợp tác: tác vụ kiểm tra cờ cancel (AtomicBoolean) giữa các bước.
 */
final class TacVu {
    private TacVu() {}

    /** Tác vụ bị người dùng hủy. */
    static final class Cancelled extends Exception {}

    interface Progress { void report(int pct, String text); }

    /** Giới hạn số tác vụ chạy cùng lúc; giới hạn đổi được khi đang chạy (thanh trượt). */
    static final class Limiter {
        private int n, running;
        Limiter(int n) { this.n = n; }

        synchronized void set(int v) {
            n = v;
            notifyAll();
        }

        synchronized boolean acquire(AtomicBoolean cancel) throws InterruptedException {
            while (running >= n) {
                if (cancel.get()) return false;
                wait(100);                                   // thức dậy định kỳ để kiểm tra cờ hủy
            }
            running++;
            return true;
        }

        synchronized void release() {
            running--;
            notifyAll();
        }
    }

    /** Tải /tep?kb=..&tre=.. qua HTTP, đọc từng phần và báo tiến trình theo Content-Length. */
    static String download(String host, int port, int kb, int delay, AtomicBoolean cancel, Progress progress)
            throws IOException, Cancelled {
        HttpURLConnection conn = (HttpURLConnection) URI.create("http://" + host + ":" + port + "/tep?kb=" + kb + "&tre=" + delay)
                .toURL().openConnection();
        conn.setConnectTimeout(3000);
        conn.setReadTimeout(3000);
        try {
            long total = conn.getContentLengthLong(), got = 0;
            InputStream in = conn.getInputStream();
            byte[] buf = new byte[16 * 1024];
            while (got < total) {
                if (cancel.get()) throw new Cancelled();
                int n = in.read(buf);                        // đọc phần đã có, tối đa 16 KB
                if (n == -1) throw new IOException("máy chủ đóng kết nối sớm");
                got += n;
                progress.report((int) (got * 100 / total), (got / 1024) + "/" + (total / 1024) + " KB");
            }
            return "xong " + (total / 1024) + " KB";
        } finally {
            conn.disconnect();                               // đóng hẳn socket (không giữ lại để dùng tiếp)
        }
    }

    /** Kiểm tra lần lượt từng cổng TCP (như Bài 1.2): kết nối được = mở. */
    static String checkPorts(String host, List<Integer> ports, AtomicBoolean cancel, Progress progress)
            throws Cancelled, InterruptedException {
        int opened = 0;
        for (int i = 0; i < ports.size(); i++) {
            if (cancel.get()) throw new Cancelled();
            int p = ports.get(i);
            boolean ok;
            try (Socket s = new Socket()) {
                s.connect(new InetSocketAddress(host, p), 300);
                ok = true;
            } catch (IOException e) {
                ok = false;
            }
            if (ok) opened++;
            Thread.sleep(300);                               // quét chậm cho dễ quan sát
            progress.report((i + 1) * 100 / ports.size(), "cổng " + p + " " + (ok ? "mở" : "đóng"));
        }
        return opened + " mở, " + (ports.size() - opened) + " đóng";
    }

    /** Tra cứu tên qua máy chủ "DNS" giả lập trên UDP, có thời gian chờ. */
    static String lookupNames(String host, int port, List<String> names, AtomicBoolean cancel, Progress progress)
            throws IOException, Cancelled {
        int found = 0;
        try (DatagramSocket s = new DatagramSocket()) {
            s.setSoTimeout(1000);
            InetSocketAddress addr = new InetSocketAddress(host, port);
            byte[] buf = new byte[512];
            for (int i = 0; i < names.size(); i++) {
                if (cancel.get()) throw new Cancelled();
                String name = names.get(i);
                byte[] q = ("A " + name).getBytes(StandardCharsets.UTF_8);
                s.send(new DatagramPacket(q, q.length, addr));
                String answer;
                try {
                    DatagramPacket p = new DatagramPacket(buf, buf.length);
                    s.receive(p);
                    answer = new String(p.getData(), 0, p.getLength(), StandardCharsets.UTF_8);
                } catch (SocketTimeoutException e) {
                    answer = name + " (hết thời gian chờ)";
                }
                if (!answer.contains("NXDOMAIN")) found++;
                progress.report((i + 1) * 100 / names.size(), answer);
            }
        }
        return found + "/" + names.size() + " tên có địa chỉ";
    }
}
