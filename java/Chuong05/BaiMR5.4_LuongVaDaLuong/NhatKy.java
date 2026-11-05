import java.io.BufferedReader;
import java.io.FileDescriptor;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStreamReader;
import java.io.OutputStreamWriter;
import java.io.PrintStream;
import java.io.PrintWriter;
import java.net.Socket;
import java.nio.charset.StandardCharsets;
import java.util.Locale;

/**
 * Bài mở rộng MR5.4 – Nhật ký theo luồng (tên luồng + biến cục bộ luồng ThreadLocal).
 * Mỗi dòng: "[  0.123 s] tải-1 yc=1      | nội dung"
 * - Thread.currentThread().getName() cho biết LUỒNG NÀO làm việc đó;
 * - ngữ cảnh "yc=..." nằm trong ThreadLocal: mỗi luồng có bản sao riêng, đặt một lần rồi mọi lời gọi log()
 *   sau đó tự có (giống MDC của SLF4J/Logback).
 * Phương thức synchronized bảo đảm hai luồng không in xen lẫn trên một dòng.
 */
final class NhatKy {
    static final PrintStream OUT = new PrintStream(new FileOutputStream(FileDescriptor.out), true, StandardCharsets.UTF_8);
    static final long T0 = System.nanoTime();
    static final ThreadLocal<String> NGU_CANH = ThreadLocal.withInitial(() -> "");   // biến cục bộ luồng

    static void datNguCanh(String s) { NGU_CANH.set(s); }

    static double bayGio() { return (System.nanoTime() - T0) / 1e9; }

    static synchronized void log(String msg) {
        String ai = (Thread.currentThread().getName() + " " + NGU_CANH.get()).trim();
        OUT.println(String.format(Locale.ROOT, "[%7.3f s] %-16s| %s", bayGio(), ai, msg));
    }

    static synchronized void say(String msg) { OUT.println(msg); }

    static void say() { say(""); }

    // Định dạng với Locale.ROOT: số thập phân luôn dùng dấu chấm, không phụ thuộc ngôn ngữ của máy
    static String f(String fmt, Object... a) { return String.format(Locale.ROOT, fmt, a); }

    // Bộ đọc/ghi dòng UTF-8 trên một socket
    static BufferedReader doc(Socket s) throws IOException {
        return new BufferedReader(new InputStreamReader(s.getInputStream(), StandardCharsets.UTF_8));
    }

    static PrintWriter ghi(Socket s) throws IOException {
        return new PrintWriter(new OutputStreamWriter(s.getOutputStream(), StandardCharsets.UTF_8), true) {
            @Override public void println(String x) { print(x + "\n"); flush(); }   // luôn dùng \n
        };
    }
}
