import 'dart:async';
import 'dart:convert';
import 'package:fluent_ui/fluent_ui.dart';
import 'appearance.dart';

typedef BackupRequest = Future<dynamic> Function(String path, {String method, Object? body});
const backupParts = <String, String>{
  'game': '角色与团录', 'config': '配置', 'plugins': '插件与数据',
  'rules': '规则', 'decks': '牌堆', 'logs': '日志文件',
};

class BackupSettings extends StatefulWidget {
  const BackupSettings({super.key, required this.request, required this.refresh, required this.notify});
  final BackupRequest request;
  final Future<void> Function() refresh;
  final Future<void> Function(String) notify;
  @override
  State<BackupSettings> createState() => _BackupSettingsState();
}

class _BackupSettingsState extends State<BackupSettings> {
  Map<String, dynamic>? config;
  Map<String, dynamic> status = {};
  final controls = <String, TextEditingController>{};
  Timer? timer;
  String? error;
  bool busy = false, dirty = false;
  @override
  void initState() {
    super.initState();
    load();
    poll();
    timer = Timer.periodic(const Duration(seconds: 10), (_) => poll());
  }
  @override
  void dispose() {
    timer?.cancel();
    for (final c in controls.values) { c.dispose(); }
    super.dispose();
  }
  Future<void> load() async {
    try {
      final result = await widget.request('/config');
      if (!mounted) return;
      final value = Map<String, dynamic>.from(result['backup']);
      for (final name in ['interval_minutes', 'keep']) {
        controls.putIfAbsent(name, () => TextEditingController()).text = '${value[name]}';
      }
      for (final name in ['webdav', 's3']) {
        for (final item in (value[name] as Map).entries) {
          if (item.value is String) {
            controls.putIfAbsent('$name.${item.key}', () => TextEditingController()).text = item.value as String;
          }
        }
      }
      setState(() { config = value; dirty = false; });
    } catch (e) { if (mounted) setState(() => error = '$e'); }
  }
  Future<void> poll() async {
    try {
      final value = await widget.request('/backups/status');
      if (mounted) setState(() => status = Map<String, dynamic>.from(value));
    } catch (_) {}
  }
  Future<void> run(Future<void> Function() work) async {
    setState(() { busy = true; error = null; });
    try { await work(); }
    catch (e) { if (mounted) setState(() => error = '$e'); }
    finally { if (mounted) setState(() => busy = false); }
  }
  void change(void Function() work) => setState(() { work(); dirty = true; });
  Widget entry(String key, String label, {bool secret = false, String? placeholder, bool number = false}) {
    final colors = WorkspacePalette.of(context);
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      Text(label, style: TextStyle(fontSize: 13, color: colors.text)),
      const SizedBox(height: 8),
      TextBox(controller: controls[key], placeholder: placeholder, obscureText: secret,
        enabled: !busy && status['running'] != true,
        keyboardType: number ? TextInputType.number : TextInputType.text,
        onChanged: (_) => change(() {})),
    ]);
  }
  Widget fields(List<Widget> children) => LayoutBuilder(builder: (context, constraints) {
    final width = constraints.maxWidth >= 520 ? (constraints.maxWidth - 16) / 2 : constraints.maxWidth;
    return Padding(padding: const EdgeInsets.symmetric(vertical: 16),
      child: Wrap(spacing: 16, runSpacing: 16, children: [for (final child in children) SizedBox(width: width, child: child)]));
  });
  Widget heading(String title, Widget action) => Row(children: [
    Expanded(child: Text(title, style: const TextStyle(fontSize: 13, fontWeight: FontWeight.w600))), action,
  ]);
  Map<String, dynamic> edited() {
    final value = Map<String, dynamic>.from(jsonDecode(jsonEncode(config)));
    value['interval_minutes'] = int.tryParse(controls['interval_minutes']!.text);
    value['keep'] = int.tryParse(controls['keep']!.text);
    if (value['interval_minutes'] == null || value['interval_minutes'] < 1 || value['interval_minutes'] > 525600) {
      throw Exception('备份间隔应为 1–525600 分钟');
    }
    if (value['keep'] == null || value['keep'] < 1 || value['keep'] > 1000) {
      throw Exception('保留数量应为 1–1000 次');
    }
    for (final name in ['webdav', 's3']) {
      for (final item in controls.entries.where((e) => e.key.startsWith('$name.'))) {
        value[name][item.key.substring(name.length + 1)] = item.value.text;
      }
    }
    return value;
  }
  Future<void> create(bool full) => run(() async {
    final result = await widget.request('/backups', method: 'POST', body: {
      'contents': full ? {for (final key in backupParts.keys) key: true} : config!['contents'],
    });
    await poll(); await widget.refresh();
    await widget.notify(result['status'] == 'partial' ? '本地备份已保存，部分上传失败' : '备份完成');
  });
  @override
  Widget build(BuildContext context) {
    if (config == null) return Text(error ?? '正在读取备份设置');
    final colors = WorkspacePalette.of(context);
    final value = config!;
    final disabled = busy || status['running'] == true;
    final selected = (value['contents'] as Map).values.any((v) => v == true);
    final last = status['last_result'] as Map? ?? {};
    final next = status['next_at'] is num ? DateTime.fromMillisecondsSinceEpoch((status['next_at'] as num).toInt() * 1000).toLocal().toString().split('.').first : '—';
    Widget separator() => Padding(padding: const EdgeInsets.symmetric(vertical: 18), child: Container(height: 1, color: colors.edge));
    return Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
      heading('自动备份', ToggleSwitch(content: const Text('启用'), checked: value['enabled'] == true,
        onChanged: disabled ? null : (v) => change(() => value['enabled'] = v))),
      fields([entry('interval_minutes', '间隔（分钟）', number: true), entry('keep', '保留自动备份（次）', number: true)]),
      heading('备份内容', Button(onPressed: disabled ? null : () => change(() => value['contents'] = {for (final key in backupParts.keys) key: true}), child: const Text('全选'))),
      const SizedBox(height: 14),
      Wrap(spacing: 24, runSpacing: 14, children: [for (final part in backupParts.entries)
        Checkbox(content: Text(part.value), checked: value['contents'][part.key] == true,
          onChanged: disabled ? null : (v) => change(() => value['contents'][part.key] = v == true)),
      ]),
      separator(),
      heading('WebDAV', ToggleSwitch(content: const Text('备份后上传'), checked: value['webdav']['enabled'] == true,
        onChanged: disabled ? null : (v) => change(() => value['webdav']['enabled'] = v))),
      if (value['webdav']['enabled'] == true) fields([
        entry('webdav.url', '备份目录地址', placeholder: 'https://example.com/dav/qianbian/'),
        entry('webdav.username', '用户名'),
        entry('webdav.password', '密码', secret: true, placeholder: value['webdav']['has_password'] == true ? '已保存，留空保持' : null),
      ]),
      separator(),
      heading('S3', ToggleSwitch(content: const Text('备份后上传'), checked: value['s3']['enabled'] == true,
        onChanged: disabled ? null : (v) => change(() => value['s3']['enabled'] = v))),
      if (value['s3']['enabled'] == true) ...[
        fields([
          entry('s3.endpoint', '服务地址', placeholder: 'https://s3.us-east-1.amazonaws.com'),
          entry('s3.region', '区域'), entry('s3.bucket', '存储桶'), entry('s3.prefix', '目录前缀'),
          entry('s3.access_key', 'Access Key'),
          entry('s3.secret_key', 'Secret Key', secret: true, placeholder: value['s3']['has_secret_key'] == true ? '已保存，留空保持' : null),
          entry('s3.session_token', '会话令牌（可选）', secret: true, placeholder: value['s3']['has_session_token'] == true ? '已保存，留空保持' : null),
        ]),
        Checkbox(content: const Text('使用路径式地址'), checked: value['s3']['path_style'] == true,
          onChanged: disabled ? null : (v) => change(() => value['s3']['path_style'] = v == true)),
      ],
      const SizedBox(height: 20),
      Wrap(spacing: 10, runSpacing: 10, children: [
        FilledButton(onPressed: disabled || !dirty || !selected ? null : () => run(() async {
          await widget.request('/config', method: 'PUT', body: {'backup': edited()});
          await load(); await poll(); await widget.notify('备份设置已保存');
        }), child: const Text('保存备份设置')),
        Button(onPressed: disabled || !selected ? null : () => create(false), child: const Text('立即备份')),
        Button(onPressed: disabled ? null : () => create(true), child: const Text('完整备份')),
      ]),
      const SizedBox(height: 12),
      Text('手动备份单独保留。上传选项保存后生效。', style: TextStyle(fontSize: 12, color: colors.muted)),
      separator(),
      Text(status['running'] == true ? '正在备份' : value['enabled'] == true ? '下次备份 $next' : '自动备份已关闭', style: TextStyle(fontSize: 13, color: colors.muted)),
      if (last['id'] != null) Padding(padding: const EdgeInsets.only(top: 8), child: Text('最近备份 ${last['id']}', style: TextStyle(fontSize: 12, color: colors.muted))),
      if (last['status'] == 'failed') Text('${last['message']}'),
      for (final item in (last['uploads'] as List? ?? [])) Padding(padding: const EdgeInsets.only(top: 8),
        child: Text('${item['target'] == 'webdav' ? 'WebDAV' : item['target'] == 's3' ? 'S3' : '备份打包'} · ${item['status'] == 'success' ? '已上传' : item['message']}', style: TextStyle(fontSize: 12, color: colors.muted))),
      for (final message in (last['warnings'] as List? ?? [])) Text('$message'),
      if (error != null) InfoBar(title: const Text('无法完成备份操作'), content: Text(error!), severity: InfoBarSeverity.error),
    ]);
  }
}
