import java.awt.BorderLayout;
import java.awt.Color;
import java.awt.Dimension;
import java.awt.FlowLayout;
import java.awt.Font;
import java.awt.GridBagConstraints;
import java.awt.GridBagLayout;
import java.awt.Insets;
import java.io.IOException;
import java.util.ArrayList;
import java.util.List;
import javax.swing.BorderFactory;
import javax.swing.BoxLayout;
import javax.swing.JButton;
import javax.swing.JCheckBox;
import javax.swing.JFrame;
import javax.swing.JLabel;
import javax.swing.JPanel;
import javax.swing.JScrollPane;
import javax.swing.JTable;
import javax.swing.JTextField;
import javax.swing.SwingConstants;
import javax.swing.SwingUtilities;
import javax.swing.Timer;
import javax.swing.table.DefaultTableCellRenderer;
import javax.swing.table.DefaultTableModel;

/**
 * Bài mở rộng MR6.5 – Máy chủ phát trực tiếp có giao diện Swing.
 *
 * Lõi MayChu chạy các luồng mạng; cửa sổ chỉ gọi phương thức của lõi và cứ 100 ms đọc trangThai() để vẽ lại. Server
 * bật xemTruoc: tự giải mã MỘT bản luồng video (chỉ để hiện ảnh xem trước, việc chuyển tiếp vẫn là nhân bản nguyên
 * byte – không giải nén).
 *
 * Chạy:  java -cp out ServerGui [host] [cổng_tcp] [cổng_udp]
 */
public class ServerGui {
    final JFrame frame = new JFrame("Máy chủ phát trực tiếp (SFU) – MR6.5");
    final MayChu lop;
    final JTextField txtIp, txtTcp, txtUdp;
    final JButton btnStart = GiaoDienChung.nut("START SERVER", GiaoDienChung.XANH_LA, 132),
            btnStop = GiaoDienChung.nut("STOP SERVER", GiaoDienChung.DO, 132);
    final JCheckBox chkBoDem = new JCheckBox("<html>Bật – người vào muộn nhận ngay<br>khung I mới nhất + các P sau nó</html>", true);
    final JLabel lblBoDem = GiaoDienChung.nhan("Đang giữ: –");
    final JLabel lblVao = GiaoDienChung.nhanDam("Vào: –"), lblRa = GiaoDienChung.nhanDam("Ra: –"),
            lblHeSo = GiaoDienChung.nhanDam("Hệ số nhân bản (ra/vào): –"), lblPli = GiaoDienChung.nhan("PLI chuyển: 0 (bỏ 0)");
    final GiaoDienChung.KhungAnh khungAnh = new GiaoDienChung.KhungAnh(2 * Chung.RONG, 2 * Chung.CAO);
    final JLabel lblDongHo = GiaoDienChung.nhan("Đồng hồ: –");
    final DefaultTableModel mdl = new DefaultTableModel(new Object[] {"Tên", "IP:cổng UDP", "Nhận", "Gói gửi", "kbit/s", "Khung I đầu"}, 0) {
        @Override public boolean isCellEditable(int r, int c) { return false; }
    };
    final JTable bang = new JTable(mdl);
    final List<Integer> maTheoHang = new ArrayList<>();               // hàng i của bảng là người xem mã maTheoHang[i]
    final JButton btnNgat = GiaoDienChung.nut("Ngắt người chọn", GiaoDienChung.CAM, 150);
    final JLabel lblPhat = GiaoDienChung.nhan("  Máy phát: –");
    final GiaoDienChung.NhatKy nhatKy = new GiaoDienChung.NhatKy(7, 100, "server");
    final JLabel thanhTt = GiaoDienChung.thanhTrangThai("Trạng thái: server chưa chạy");
    final GiaoDienChung.ThongKeDinhKy dinhKy = new GiaoDienChung.ThongKeDinhKy("server");
    final Timer hen = new Timer(100, e -> capNhat());
    Object daVe;                                                       // (ts, số) của ảnh xem trước đã vẽ

    ServerGui(String host, int congTcp, int congUdp) {
        lop = new MayChu(t -> SwingUtilities.invokeLater(() -> nhatKy.ghi(t)));
        lop.xemTruoc = true;
        txtIp = GiaoDienChung.oNhap(host, 12);
        txtTcp = GiaoDienChung.oNhap(congTcp, 5);
        txtUdp = GiaoDienChung.oNhap(congUdp > 0 ? congUdp : congTcp + 1, 5);

        JPanel tren = new JPanel();
        tren.setLayout(new BoxLayout(tren, BoxLayout.X_AXIS));
        tren.setBackground(GiaoDienChung.NEN);
        JPanel g = GiaoDienChung.khungNhom("Kết nối");
        g.setLayout(new GridBagLayout());
        GridBagConstraints c = new GridBagConstraints();
        c.anchor = GridBagConstraints.WEST;
        c.insets = new Insets(1, 2, 1, 2);
        c.gridx = 0; c.gridy = 0; g.add(GiaoDienChung.nhan("IP:"), c);
        c.gridx = 1; c.gridwidth = 3; g.add(txtIp, c);
        c.gridwidth = 1; c.gridx = 0; c.gridy = 1; g.add(GiaoDienChung.nhan("TCP:"), c);
        c.gridx = 1; g.add(txtTcp, c);
        c.gridx = 2; g.add(GiaoDienChung.nhan("UDP:"), c);
        c.gridx = 3; g.add(txtUdp, c);
        c.gridy = 2; c.gridx = 0; c.gridwidth = 2; c.insets = new Insets(4, 2, 1, 2); g.add(btnStart, c);
        c.gridx = 2; g.add(btnStop, c);
        g.setMinimumSize(g.getPreferredSize());
        g.setMaximumSize(g.getPreferredSize());
        tren.add(g);
        g = GiaoDienChung.khungNhom("Bộ đệm GOP (GOP cache)");
        g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
        chkBoDem.setBackground(GiaoDienChung.NEN);
        g.add(chkBoDem);
        lblBoDem.setText("Đang giữ: I #000 và các P sau: 00 gói, 00 KB");
        lblBoDem.setPreferredSize(lblBoDem.getPreferredSize());     // chữ đổi khi chạy: giữ cố định bố cục
        lblBoDem.setText("Đang giữ: –");
        g.add(lblBoDem);
        g.setMinimumSize(g.getPreferredSize());
        tren.add(g);
        g = GiaoDienChung.khungNhom("Chỉ số (mỗi giây)");
        g.setLayout(new BoxLayout(g, BoxLayout.Y_AXIS));
        lblHeSo.setForeground(new Color(0x003a80));
        for (JLabel l : new JLabel[] {lblVao, lblRa, lblHeSo, lblPli}) g.add(l);
        g.setPreferredSize(new Dimension(330, g.getPreferredSize().height));
        tren.add(g);

        JPanel giua = new JPanel();
        giua.setLayout(new BoxLayout(giua, BoxLayout.X_AXIS));
        giua.setBackground(GiaoDienChung.NEN);
        g = GiaoDienChung.khungNhom("Xem trước luồng (server tự giải mã)");
        g.setLayout(new BorderLayout(0, 2));
        khungAnh.datChu("Chưa có luồng");
        g.add(khungAnh, BorderLayout.CENTER);
        lblDongHo.setFont(new Font(Font.MONOSPACED, Font.PLAIN, 12));
        g.add(lblDongHo, BorderLayout.SOUTH);
        g.setMaximumSize(g.getPreferredSize());
        giua.add(g);
        g = GiaoDienChung.khungNhom("Người xem");
        g.setLayout(new BorderLayout(0, 4));
        int[] rong = {60, 130, 45, 65, 55, 130};
        for (int i = 0; i < rong.length; i++) bang.getColumnModel().getColumn(i).setPreferredWidth(rong[i]);
        DefaultTableCellRenderer phai = new DefaultTableCellRenderer();
        phai.setHorizontalAlignment(SwingConstants.RIGHT);
        for (int i : new int[] {2, 3, 4}) bang.getColumnModel().getColumn(i).setCellRenderer(phai);
        JScrollPane cuon = new JScrollPane(bang);
        cuon.setPreferredSize(new Dimension(500, 200));
        g.add(cuon, BorderLayout.CENTER);
        JPanel f = new JPanel(new FlowLayout(FlowLayout.LEFT, 0, 0));
        f.setBackground(GiaoDienChung.NEN);
        f.add(btnNgat);
        f.add(lblPhat);
        g.add(f, BorderLayout.SOUTH);
        giua.add(g);

        g = GiaoDienChung.khungNhom("Nhật ký");
        g.setLayout(new BorderLayout());
        g.add(nhatKy, BorderLayout.CENTER);
        JPanel duoi = new JPanel(new BorderLayout(0, 4));
        duoi.setBackground(GiaoDienChung.NEN);
        duoi.add(g, BorderLayout.CENTER);
        duoi.add(thanhTt, BorderLayout.SOUTH);

        JPanel goc = new JPanel(new BorderLayout(0, 4));
        goc.setBackground(GiaoDienChung.NEN);
        goc.setBorder(BorderFactory.createEmptyBorder(4, 6, 6, 6));
        goc.add(tren, BorderLayout.NORTH);
        goc.add(giua, BorderLayout.CENTER);
        goc.add(duoi, BorderLayout.SOUTH);
        frame.setContentPane(goc);
        frame.setDefaultCloseOperation(JFrame.DO_NOTHING_ON_CLOSE);
        frame.addWindowListener(new java.awt.event.WindowAdapter() {
            @Override public void windowClosing(java.awt.event.WindowEvent e) { dong(); System.exit(0); }
        });
        frame.setResizable(false);
        frame.pack();

        btnStop.setEnabled(false);
        btnStart.addActionListener(e -> batDau());
        btnStop.addActionListener(e -> dung());
        btnNgat.addActionListener(e -> ngatChon());
        chkBoDem.addActionListener(e -> lop.datBoDem(chkBoDem.isSelected()));
        hen.start();
    }

    // ------------------------------------------------------------ nút
    void batDau() {
        try {
            lop.boDemBat = chkBoDem.isSelected();
            lop.batDau(txtIp.getText().strip(), Integer.parseInt(txtTcp.getText().strip()), Integer.parseInt(txtUdp.getText().strip()));
        } catch (IOException | RuntimeException e) {
            nhatKy.ghi("Không khởi động được: " + e.getMessage());
            return;
        }
        btnStart.setEnabled(false);
        btnStop.setEnabled(true);
        for (JTextField o : new JTextField[] {txtIp, txtTcp, txtUdp}) o.setEnabled(false);
    }

    void dung() {
        btnStop.setEnabled(false);
        Chung.chayNen("dung-server", () -> {                         // chờ luồng nền dừng: không chặn giao diện
            lop.dung();
            SwingUtilities.invokeLater(() -> {
                btnStart.setEnabled(true);
                for (JTextField o : new JTextField[] {txtIp, txtTcp, txtUdp}) o.setEnabled(true);
            });
        });
    }

    void ngatChon() {
        for (int r : bang.getSelectedRows()) if (r < maTheoHang.size()) lop.ngat(null, maTheoHang.get(r));
    }

    /** Chọn dòng theo tên (dùng trong kịch bản demo). */
    void chonNguoiXem(String ten) {
        for (int r = 0; r < mdl.getRowCount(); r++) if (ten.equals(mdl.getValueAt(r, 0))) bang.setRowSelectionInterval(r, r);
    }

    // ------------------------------------------------------------ vẽ lại định kỳ
    void capNhat() {
        MayChu.TrangThai tt = lop.trangThai();
        if (lop.chay) {
            dinhKy.inNeuDenHan(() -> {
                StringBuilder b = new StringBuilder(Chung.f("vào %.0f kbit/s (tiếng %.0f, hình %.0f), ra %.0f, hệ số %.2f, %s;",
                        tt.kbpsVao, tt.kbpsVaoAm, tt.kbpsVaoHinh, tt.kbpsRa, tt.heSo, tt.dangPhat ? "đang phát" : "chưa phát"));
                for (MayChu.TomTat x : tt.nguoiXem) b.append(Chung.f(" %s %s %.0f", x.ten(), x.nhan(), x.kbps()));
                return b.toString();
            });
            lblVao.setText(Chung.f("Vào: %.0f kbit/s (tiếng %.0f, hình %.0f)", tt.kbpsVao, tt.kbpsVaoAm, tt.kbpsVaoHinh));
            lblRa.setText(Chung.f("Ra: %.0f kbit/s tới %d người xem", tt.kbpsRa, tt.nguoiXem.size()));
            lblHeSo.setText(Chung.f("Hệ số nhân bản (ra/vào): %.2f", tt.heSo));
            lblPli.setText("PLI chuyển: " + tt.pliChuyen + " (bỏ " + tt.pliBo + " do lọc 500 ms)");
            lblBoDem.setText(tt.boDemBat && tt.boDemSoI != null
                    ? Chung.f("Đang giữ: I #%d và các P sau: %d gói, %.0f KB", tt.boDemSoI, tt.boDemGoi, tt.boDemByte / 1024.0)
                    : tt.boDemBat ? "Đang giữ: – (chờ khung I)" : "Tắt: người vào muộn chờ khung I");
            lblPhat.setText("  Máy phát: " + (tt.tenPhat == null ? "–" : tt.tenPhat) + (tt.dangPhat ? " (đang phát)" : ""));
            thanhTt.setText("Trạng thái: server chạy tại " + lop.diaChi + " (UDP " + lop.congUdp + ") – "
                    + (tt.dangPhat ? "đang phát" : "chưa phát") + ", " + tt.nguoiXem.size() + " người xem");
        } else {
            thanhTt.setText("Trạng thái: server chưa chạy");
        }
        // bảng người xem: cập nhật tại chỗ để giữ dòng đang chọn
        List<Integer> moi = new ArrayList<>();
        for (MayChu.TomTat x : tt.nguoiXem) moi.add(x.ma());
        if (!moi.equals(maTheoHang)) {
            mdl.setRowCount(0);
            maTheoHang.clear();
            for (MayChu.TomTat x : tt.nguoiXem) {
                mdl.addRow(new Object[6]);
                maTheoHang.add(x.ma());
            }
        }
        for (int r = 0; r < tt.nguoiXem.size(); r++) {
            MayChu.TomTat x = tt.nguoiXem.get(r);
            Object[] gt = {x.ten(), x.udp(), x.nhan(), x.goiRa(), Chung.f("%.0f", x.kbps()),
                    x.khungDauMs() == null ? "–" : Chung.f("%.0f ms (%s)", x.khungDauMs(), x.nguonDau())};
            for (int k = 0; k < gt.length; k++) if (!String.valueOf(gt[k]).equals(String.valueOf(mdl.getValueAt(r, k)))) mdl.setValueAt(gt[k], r, k);
        }
        MayChu.AnhXemTruoc a = tt.anh;
        if (a != null && lop.chay) {
            List<Long> khoa = List.of(a.ts(), a.so());
            if (!khoa.equals(daVe)) {
                khungAnh.datAnh(GiaoDienChung.anh(a.rgb(), a.w(), a.h()));
                lblDongHo.setText(Chung.f("Đồng hồ phương tiện: t=%7.3f s, khung #%d", a.ts() / 1000.0, a.so()));
                daVe = khoa;
            }
        } else if (a == null) {
            khungAnh.datAnh(null);
            khungAnh.datChu("Chưa có luồng");
        }
    }

    void dong() {
        hen.stop();
        lop.dung();
        frame.dispose();
    }

    public static void main(String[] args) {
        String host = args.length > 0 ? args[0] : "127.0.0.1";
        int tcp = args.length > 1 ? Integer.parseInt(args[1]) : 43652;
        int udp = args.length > 2 ? Integer.parseInt(args[2]) : tcp + 1;
        GiaoDienChung.NhatKy.inRa = true;                           // chạy riêng: nhật ký cũng ra dòng lệnh
        SwingUtilities.invokeLater(() -> {
            GiaoDienChung.caiDat();
            new ServerGui(host, tcp, udp).frame.setVisible(true);
        });
    }
}
