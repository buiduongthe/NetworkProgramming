import java.awt.BasicStroke;
import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.Graphics;
import java.awt.Graphics2D;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.awt.Rectangle;
import java.awt.RenderingHints;
import java.awt.Robot;
import java.awt.image.BufferedImage;
import java.io.File;
import java.io.IOException;
import java.time.LocalTime;
import java.util.ArrayList;
import java.util.LinkedHashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.Callable;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;
import java.util.function.BooleanSupplier;
import javax.imageio.ImageIO;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JComponent;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JProgressBar;
import javax.swing.JScrollPane;
import javax.swing.JSlider;
import javax.swing.JSpinner;
import javax.swing.JTextArea;
import javax.swing.SpinnerNumberModel;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.WindowConstants;

/**
 * Bài mở rộng MR5.3 – Giao diện luôn phản hồi khi nhiều tác vụ mạng chạy song song (Swing).
 *
 * Bài mở rộng, không có trong giáo trình; nối tiếp Bài 5.3 (giao diện bất đồng bộ), Bài 5.5 (tải tệp có
 * tiến trình và hủy), Bài 5.2 (giới hạn đồng thời), Bài 1.2 (kiểm tra cổng), Bài 4.3 (tra cứu tên qua UDP).
 *
 * Quy tắc vàng của lập trình giao diện: CHỈ luồng giao diện (Event Dispatch Thread – EDT) được chạm vào
 * thành phần Swing, và luồng giao diện KHÔNG BAO GIỜ được chờ mạng. Vì vậy:
 *  - mỗi tác vụ chạy trên một luồng nền (luồng ảo), xin lượt qua bộ giới hạn đồng thời (thanh trượt);
 *  - luồng nền chỉ gửi cập nhật về EDT bằng SwingUtilities.invokeLater (UI_INVOKE của Bài 5.3);
 *  - đồng hồ và hoạt hình chạy bằng javax.swing.Timer 20 ms; khoảng cách thật giữa hai nhịp được đo lại –
 *    nếu EDT bị chặn, khoảng cách này tăng vọt (giao diện "đứng").
 * Nút "Làm SAI" chạy đúng việc tải tệp đó ngay trên EDT để thấy giao diện đứng bao lâu.
 *
 * Chạy: java -cp out UngDung <host> <port> [--demo]
 *       (--demo: tự bấm các nút theo kịch bản, chụp cửa sổ bằng java.awt.Robot thành ket_qua_gui.png rồi thoát;
 *        trên Linux không màn hình: xvfb-run -a java -cp out UngDung 127.0.0.1 9000 --demo)
 */
public class UngDung {
    static final int TICK_MS = 20;
    static final int[][] FILES = {{256, 60}, {256, 90}, {384, 70}, {256, 120}, {320, 50}, {256, 100}, {192, 80}, {256, 40}};
    static final List<String> NAMES = List.of("router.lab", "may-in.lab", "camera-1.lab", "nas.lab", "khong-co.lab");

    interface Job { String run(AtomicBoolean cancel, TacVu.Progress progress) throws Exception; }

    /** Một tác vụ và các thành phần của nó trên cửa sổ. */
    static final class Task {
        final int id;
        final String name;
        final AtomicBoolean cancel = new AtomicBoolean();
        volatile boolean done;
        JLabel lbl, status;
        JProgressBar bar;
        JButton btn;
        Task(int id, String name) { this.id = id; this.name = name; }
    }

    final String host;
    final int port;
    final TacVu.Limiter limiter = new TacVu.Limiter(3);
    final List<Task> tasks = new ArrayList<>();                   // chỉ EDT đọc/ghi
    int nextId = 1;
    final long t0 = System.nanoTime();
    long last = t0;
    int ticks = 0;
    final Map<String, double[]> gaps = new LinkedHashMap<>();     // giai đoạn -> {số nhịp, tổng, lớn nhất} (ms)
    boolean badPending = false;

    final JFrame frame = new JFrame("MR5.3 – Giao diện đa tác vụ (Swing)");
    final JLabel lblClock = new JLabel();
    final JLabel lblStats = new JLabel();
    final Anim anim = new Anim();
    final JSpinner spinN = new JSpinner(new SpinnerNumberModel(5, 1, FILES.length, 1));
    final JButton btnDl = new JButton("Tải tệp"), btnPorts = new JButton("Kiểm tra cổng"), btnDns = new JButton("Tra cứu tên");
    final JSlider slider = new JSlider(1, 8, 3);
    final JButton btnCancelAll = new JButton("Hủy tất cả"), btnBad = new JButton("Làm SAI (tải trên luồng GUI)");
    final JPanel pnlTasks = new JPanel(new GridBagLayout());
    final JTextArea txtLog = new JTextArea(8, 80);

    /** Hoạt hình: quả bóng chạy qua lại và cung tròn quay – đứng yên ngay khi EDT bị chặn. */
    static final class Anim extends JComponent {
        double t;
        Anim() { setPreferredSize(new Dimension(260, 36)); }
        @Override protected void paintComponent(Graphics g0) {
            Graphics2D g = (Graphics2D) g0;
            g.setRenderingHint(RenderingHints.KEY_ANTIALIASING, RenderingHints.VALUE_ANTIALIAS_ON);
            g.setColor(new Color(0xF4, 0xF6, 0xFA));
            g.fillRect(0, 0, getWidth(), getHeight());
            int x = (int) (110 + 100 * Math.sin(t * 3));
            g.setColor(new Color(0x24, 0x57, 0xC5));
            g.fillOval(x, 8, 20, 20);
            g.setColor(new Color(0xE0, 0x7B, 0x00));
            g.setStroke(new BasicStroke(4));
            g.drawArc(222, 2, 32, 32, (int) ((t * 360) % 360), 270);
        }
    }

    UngDung(String host, int port) {
        this.host = host;
        this.port = port;
        build();
        new Timer(TICK_MS, e -> tick()).start();                  // nhịp đồng hồ trên EDT
    }

    // ---------- dựng giao diện ----------
    void build() {
        lblClock.setFont(new Font(Font.MONOSPACED, Font.BOLD, 22));
        JPanel top = new JPanel(new FlowLayout(FlowLayout.LEFT, 12, 4));
        top.add(lblClock);
        top.add(anim);
        lblStats.setBorder(BorderFactory.createEmptyBorder(0, 8, 0, 8));
        lblStats.setText(statsText());

        JPanel bar = new JPanel(new FlowLayout(FlowLayout.LEFT, 6, 4));
        bar.add(spinN);
        bar.add(btnDl);
        bar.add(btnPorts);
        bar.add(btnDns);
        bar.add(new JLabel("   Giới hạn đồng thời"));
        slider.setMajorTickSpacing(1);
        slider.setSnapToTicks(true);
        slider.setPaintLabels(true);
        slider.setPreferredSize(new Dimension(150, 44));
        slider.addChangeListener(e -> limiter.set(slider.getValue()));
        bar.add(slider);
        bar.add(btnCancelAll);
        btnBad.setBackground(new Color(0xF6, 0xD5, 0xD9));
        bar.add(btnBad);
        btnDl.addActionListener(e -> startDownloads());
        btnPorts.addActionListener(e -> startPorts());
        btnDns.addActionListener(e -> startDns());
        btnCancelAll.addActionListener(e -> cancelAll());
        btnBad.addActionListener(e -> bad());

        pnlTasks.setBorder(BorderFactory.createTitledBorder("Tác vụ"));
        txtLog.setEditable(false);
        txtLog.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12));
        JScrollPane sp = new JScrollPane(txtLog);
        sp.setBorder(BorderFactory.createTitledBorder("Nhật ký"));

        JPanel north = new JPanel();
        north.setLayout(new BoxLayout(north, BoxLayout.Y_AXIS));
        for (JComponent c : new JComponent[] {top, lblStats, bar, pnlTasks}) {
            c.setAlignmentX(0f);
            north.add(c);
        }
        JPanel root = new JPanel(new BorderLayout(4, 4));
        root.setBorder(BorderFactory.createEmptyBorder(4, 8, 8, 8));
        root.add(north, BorderLayout.NORTH);
        root.add(sp, BorderLayout.CENTER);
        frame.setContentPane(root);
        frame.setDefaultCloseOperation(WindowConstants.DISPOSE_ON_CLOSE);
        frame.addWindowListener(new java.awt.event.WindowAdapter() {
            @Override public void windowClosed(java.awt.event.WindowEvent e) { close(); }
        });
        frame.setSize(1000, 700);
        frame.setLocation(0, 0);
    }

    // ---------- nhịp đồng hồ: phải luôn chạy đều ----------
    void tick() {
        long now = System.nanoTime();
        double gap = (now - last) / 1e6;
        last = now;
        if (ticks > 0) {
            String phase;
            if (badPending) {
                phase = "luồng GUI bị chặn";
                badPending = false;
            } else if (tasks.stream().anyMatch(t -> !t.done)) phase = "chạy song song";
            else phase = "rỗi";
            double[] g = gaps.computeIfAbsent(phase, k -> new double[3]);
            g[0]++;
            g[1] += gap;
            g[2] = Math.max(g[2], gap);
        }
        ticks++;
        LocalTime lt = LocalTime.now();
        lblClock.setText(String.format("%02d:%02d:%02d.%03d", lt.getHour(), lt.getMinute(), lt.getSecond(), lt.getNano() / 1_000_000));
        anim.t = now / 1e9;
        anim.repaint();
        lblStats.setText(statsText());
    }

    String statsText() {
        StringBuilder b = new StringBuilder("<html>Khoảng cách thật giữa hai nhịp đồng hồ (đặt 20 ms):");
        boolean any = false;
        for (String phase : new String[] {"chạy song song", "luồng GUI bị chặn"}) {
            double[] g = gaps.get(phase);
            if (g == null) continue;
            any = true;
            b.append(String.format(Locale.ROOT, "<br>&nbsp;&nbsp;• %s: tối đa %.0f ms, trung bình %.1f ms (%d nhịp)", phase, g[2], g[1] / g[0], (int) g[0]));
        }
        if (!any) b.append("<br>&nbsp;&nbsp;đang đo…");
        return b.append("</html>").toString();
    }

    void log(String text) {
        String line = String.format(Locale.ROOT, "%6.2f s  %s", (System.nanoTime() - t0) / 1e9, text);
        System.out.println("[gui] " + line);
        txtLog.append(line + "\n");
        txtLog.setCaretPosition(txtLog.getDocument().getLength());
    }

    // ---------- tạo và chạy tác vụ ----------
    void clearFinished() {
        tasks.removeIf(t -> {
            if (!t.done) return false;
            for (JComponent c : new JComponent[] {t.lbl, t.bar, t.status, t.btn}) pnlTasks.remove(c);
            return true;
        });
        relayoutTasks();
    }

    void relayoutTasks() {                                         // xếp lại các hàng còn lại lên trên
        GridBagLayout gl = (GridBagLayout) pnlTasks.getLayout();
        for (int row = 0; row < tasks.size(); row++) {
            Task t = tasks.get(row);
            JComponent[] cs = {t.lbl, t.bar, t.status, t.btn};
            for (int col = 0; col < cs.length; col++) {
                GridBagConstraints c = gl.getConstraints(cs[col]);
                c.gridy = row;
                gl.setConstraints(cs[col], c);
            }
        }
        pnlTasks.revalidate();
        pnlTasks.repaint();
    }

    Task spawn(String name, Job job) {
        Task t = new Task(nextId++, name);
        int row = tasks.size();
        t.lbl = new JLabel("#" + t.id + " " + name);
        t.lbl.setPreferredSize(new Dimension(210, 26));
        t.bar = new JProgressBar(0, 100);
        t.bar.setPreferredSize(new Dimension(300, 18));
        t.status = new JLabel("chờ lượt");
        t.status.setPreferredSize(new Dimension(300, 26));
        t.btn = new JButton("Hủy");
        t.btn.addActionListener(e -> t.cancel.set(true));
        JComponent[] cs = {t.lbl, t.bar, t.status, t.btn};
        for (int col = 0; col < cs.length; col++) {
            GridBagConstraints c = new GridBagConstraints();
            c.gridx = col;
            c.gridy = row;
            c.insets = new Insets(1, 3, 1, 3);
            c.anchor = GridBagConstraints.WEST;
            pnlTasks.add(cs[col], c);
        }
        tasks.add(t);
        pnlTasks.revalidate();
        Thread.ofVirtual().start(() -> runTask(t, job));
        return t;
    }

    /** Chạy trên luồng nền: xin lượt, làm việc, báo kết quả về EDT bằng invokeLater. */
    void runTask(Task t, Job job) {
        TacVu.Progress progress = (pct, text) -> SwingUtilities.invokeLater(() -> {
            t.bar.setValue(pct);
            t.status.setText(text);
        });
        try {
            if (!limiter.acquire(t.cancel)) {
                finish(t, "đã hủy (khi đang chờ lượt)");
                return;
            }
        } catch (InterruptedException e) {
            finish(t, "đã hủy (khi đang chờ lượt)");
            return;
        }
        try {
            SwingUtilities.invokeLater(() -> t.status.setText("đang chạy…"));
            finish(t, job.run(t.cancel, progress));
        } catch (TacVu.Cancelled e) {
            finish(t, "đã hủy");
        } catch (Exception e) {
            finish(t, "lỗi: " + e.getMessage());
        } finally {
            limiter.release();
        }
    }

    void finish(Task t, String text) {
        SwingUtilities.invokeLater(() -> {
            t.done = true;
            t.status.setText(text);
            t.btn.setEnabled(false);
            log(t.name + ": " + text);
        });
    }

    void startDownloads() {
        clearFinished();
        int n = (Integer) spinN.getValue();
        log("Bấm Tải tệp: " + n + " tệp, giới hạn đồng thời " + slider.getValue());
        for (int i = 0; i < n; i++) {
            int kb = FILES[i][0], delay = FILES[i][1];
            spawn("Tải " + kb + " KB (" + delay + " ms/khối)", (c, p) -> TacVu.download(host, port, kb, delay, c, p));
        }
    }

    void startPorts() {
        clearFinished();
        List<Integer> ports = List.of(port, port + 1, port + 2, port + 3);
        log("Bấm Kiểm tra cổng: " + port + ", " + (port + 1) + ", " + (port + 2) + ", " + (port + 3));
        spawn("Kiểm tra 4 cổng", (c, p) -> TacVu.checkPorts(host, ports, c, p));
    }

    void startDns() {
        clearFinished();
        log("Bấm Tra cứu tên: " + NAMES.size() + " tên qua UDP");
        spawn("Tra cứu " + NAMES.size() + " tên", (c, p) -> TacVu.lookupNames(host, port, NAMES, c, p));
    }

    void cancelAll() {
        List<Task> running = tasks.stream().filter(t -> !t.done).toList();
        log("Bấm Hủy tất cả: " + running.size() + " tác vụ");
        for (Task t : running) t.cancel.set(true);
    }

    /** CÁCH SAI: chờ mạng ngay trên luồng giao diện – cả cửa sổ đứng cho tới khi xong. */
    void bad() {
        log("Bấm Làm SAI: tải 256 KB ngay trên luồng giao diện…");
        long t = System.nanoTime();
        try {
            TacVu.download(host, port, 256, 80, new AtomicBoolean(), (pct, text) -> { });
        } catch (IOException | TacVu.Cancelled e) {
            log("lỗi: " + e.getMessage());
        }
        badPending = true;
        log(String.format(Locale.ROOT, "Luồng giao diện bị chặn %.0f ms: đồng hồ, nút, thanh tiến trình đứng yên", (System.nanoTime() - t) / 1e6));
    }

    void close() {
        for (Task t : tasks) t.cancel.set(true);
    }

    // ---------------- kịch bản minh họa tự động (--demo) ----------------
    /** Chạy f trên EDT và chờ kết quả (thao tác điều khiển như người dùng). */
    static <T> T ui(Callable<T> f) throws Exception {
        AtomicReference<T> r = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> {
            try { r.set(f.call()); } catch (Exception e) { throw new RuntimeException(e); }
        });
        return r.get();
    }

    void waitUntil(BooleanSupplier cond) throws Exception {
        for (int i = 0; i < 600; i++) {
            if (ui(cond::getAsBoolean)) return;
            Thread.sleep(50);
        }
        throw new IllegalStateException("Kịch bản chờ quá lâu");
    }

    BufferedImage snapshot() throws Exception {
        Rectangle r = ui(() -> new Rectangle(frame.getLocationOnScreen(), frame.getSize()));
        return new Robot().createScreenCapture(r);                 // chụp màn hình ngoài EDT: không chặn giao diện
    }

    void demo(String out) throws Exception {
        Thread.sleep(600);
        ui(() -> {
            slider.setValue(3);
            spinN.setValue(5);
            btnDl.doClick(0);
            btnPorts.doClick(0);
            btnDns.doClick(0);
            return null;
        });
        Thread.sleep(900);
        ui(() -> {
            log("Kịch bản bấm Hủy ở tác vụ #" + tasks.get(1).id);
            tasks.get(1).btn.doClick(0);
            return null;
        });
        Thread.sleep(600);
        ui(() -> {
            long running = tasks.stream().filter(t -> !t.done).count();
            long waiting = tasks.stream().filter(t -> t.status.getText().equals("chờ lượt")).count();
            log("Chụp ảnh 1: " + running + " tác vụ chưa xong, " + waiting + " đang chờ lượt");
            return null;
        });
        BufferedImage a = snapshot();
        waitUntil(() -> tasks.stream().allMatch(t -> t.done));
        ui(() -> { spinN.setValue(4); btnDl.doClick(0); return null; });   // bốn tệp chạy nền trong lúc làm sai
        Thread.sleep(300);
        ui(() -> { btnBad.doClick(0); return null; });                    // chặn EDT ~1,3 s; invokeAndWait chờ xong
        Thread.sleep(100);
        BufferedImage b = snapshot();
        waitUntil(() -> tasks.stream().allMatch(t -> t.done));
        ui(() -> { spinN.setValue(4); btnDl.doClick(0); return null; });
        Thread.sleep(500);
        ui(() -> { btnCancelAll.doClick(0); return null; });
        waitUntil(() -> tasks.stream().allMatch(t -> t.done));
        Thread.sleep(200);
        BufferedImage both = new BufferedImage(Math.max(a.getWidth(), b.getWidth()) + 12, a.getHeight() + b.getHeight() + 18,
                BufferedImage.TYPE_INT_RGB);
        Graphics2D g = both.createGraphics();
        g.setColor(new Color(0xD0, 0xD4, 0xDC));
        g.fillRect(0, 0, both.getWidth(), both.getHeight());
        g.drawImage(a, 6, 6, null);
        g.drawImage(b, 6, a.getHeight() + 12, null);
        g.dispose();
        ImageIO.write(both, "png", new File(out));
        ui(() -> {
            log("Đã chụp cửa sổ: " + out);
            for (Map.Entry<String, double[]> e : gaps.entrySet()) {
                double[] v = e.getValue();
                log(String.format(Locale.ROOT, "Nhịp %-17s: %4d nhịp, TB %6.1f ms, tối đa %6.0f ms", e.getKey(), (int) v[0], v[1] / v[0], v[2]));
            }
            frame.dispose();
            return null;
        });
    }

    public static void main(String[] args) throws Exception {
        if (args.length < 2) {
            System.err.println("Cách dùng: java -cp out UngDung <host> <port> [--demo]");
            System.exit(1);
        }
        AtomicReference<UngDung> app = new AtomicReference<>();
        SwingUtilities.invokeAndWait(() -> {
            app.set(new UngDung(args[0], Integer.parseInt(args[1])));
            app.get().frame.setVisible(true);
        });
        if (args.length > 2 && args[2].equals("--demo")) {
            app.get().demo("ket_qua_gui.png");
            System.exit(0);
        }
    }
}
