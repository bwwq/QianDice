import 'dart:async';
import 'dart:convert';
import 'dart:typed_data';
import 'package:flutter/foundation.dart';
import 'package:flutter/material.dart';
import 'package:http/http.dart' as http;
import 'platform_web.dart' if (dart.library.io) 'platform_native.dart' as platform;
import 'web_client_stub.dart' if (dart.library.js_interop) 'web_client.dart' as client;

Future<void> main(List<String> args) async {
  WidgetsFlutterBinding.ensureInitialized();
  final connection = await platform.connection(args);
  runApp(QianbianApp(api: Api(connection.$1, connection.$2)));
}

class Api {
  Api(String url, this.token) : base = url.isEmpty ? Uri.base.origin : url;
  String base;
  String token;
  final http.Client _client = client.createClient();
  Uri uri(String path) => Uri.parse('$base/api/v1$path');
  Future<Uint8List> download(String path) async { final response = await _client.get(uri(path), headers: {if (!kIsWeb && token.isNotEmpty) 'authorization': 'Bearer $token'}); if (response.statusCode != 200) throw Exception('导出失败 (${response.statusCode})'); return response.bodyBytes; }
  Future<dynamic> request(String path, {String method = 'GET', Object? body}) async {
    final request = http.Request(method, uri(path));
    request.headers.addAll({'content-type': 'application/json', 'x-qianbian': '1', if (!kIsWeb && token.isNotEmpty) 'authorization': 'Bearer $token'});
    if (body != null) request.body = jsonEncode(body);
    final response = await http.Response.fromStream(await _client.send(request));
    if (response.statusCode == 401) throw Exception('请登录或检查管理令牌');
    dynamic value;
    try { value = jsonDecode(response.body); } catch (_) { value = response.body; }
    if (response.statusCode >= 400) throw Exception(value is Map ? value['error'] ?? '请求失败' : '请求失败 (${response.statusCode})');
    return value;
  }
  Stream<Map<String, dynamic>> events() async* {
    final request = http.Request('GET', uri('/events'));
    if (!kIsWeb && token.isNotEmpty) request.headers['authorization'] = 'Bearer $token';
    final response = await _client.send(request);
    if (response.statusCode != 200) return;
    await for (final line in response.stream.transform(utf8.decoder).transform(const LineSplitter())) {
      if (line.startsWith('data:')) {
        try { yield jsonDecode(line.substring(5).trim()) as Map<String, dynamic>; } catch (_) {}
      }
    }
  }
}

class QianbianApp extends StatefulWidget {
  const QianbianApp({super.key, required this.api});
  final Api api;
  @override State<QianbianApp> createState() => _QianbianAppState();
}
class _QianbianAppState extends State<QianbianApp> {
  ThemeMode mode = ThemeMode.system;
  bool loggedIn = false;
  @override void initState() { super.initState(); _resume(); }
  Future<void> _resume() async { try { final status = await widget.api.request('/status'); if (status['api'] != 1) throw Exception('管理接口版本不兼容'); if (mounted) setState(() => loggedIn = true); } catch (_) {} }
  ThemeData theme(Brightness brightness) {
    final dark = brightness == Brightness.dark;
    final scheme = ColorScheme.fromSeed(seedColor: const Color(0xffa1b8a6), brightness: brightness, surface: dark ? const Color(0xff181b1d) : const Color(0xfff4f3ef));
    return ThemeData(useMaterial3: true, colorScheme: scheme, fontFamily: 'QianbianSans', scaffoldBackgroundColor: scheme.surface,
      inputDecorationTheme: InputDecorationTheme(filled: true, fillColor: scheme.surfaceContainerHighest.withValues(alpha: .5), border: OutlineInputBorder(borderRadius: BorderRadius.circular(10), borderSide: BorderSide.none)),
      cardTheme: CardThemeData(elevation: 0, margin: EdgeInsets.zero, color: scheme.surfaceContainerLow, shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(14))),
      filledButtonTheme: FilledButtonThemeData(style: FilledButton.styleFrom(padding: const EdgeInsets.symmetric(horizontal: 22, vertical: 18), shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(9)))));
  }
  @override Widget build(BuildContext context) => MaterialApp(title: '千变', debugShowCheckedModeBanner: false, theme: theme(Brightness.light), darkTheme: theme(Brightness.dark), themeMode: mode,
    home: loggedIn ? Workspace(api: widget.api, themeMode: mode, onTheme: (m) => setState(() => mode = m), onLogout: () => setState(() => loggedIn = false)) : Login(api: widget.api, onLogin: () => setState(() => loggedIn = true)));
}

class Login extends StatefulWidget {
  const Login({super.key, required this.api, required this.onLogin}); final Api api; final VoidCallback onLogin;
  @override State<Login> createState() => _LoginState();
}
class _LoginState extends State<Login> {
  late final TextEditingController address = TextEditingController(text: widget.api.base);
  final secret = TextEditingController(); bool busy = false; String? error;
  @override void dispose() { address.dispose(); secret.dispose(); super.dispose(); }
  Future<void> submit() async {
    setState(() { busy = true; error = null; });
    try { widget.api.base = address.text.trim().replaceAll(RegExp(r'/$'), ''); widget.api.token = secret.text.trim(); await widget.api.request('/login', method: 'POST', body: {'token': widget.api.token}); final status = await widget.api.request('/status'); if (status['api'] != 1) throw Exception('管理端与后台版本不兼容'); widget.onLogin(); }
    catch (e) { setState(() => error = '$e'); }
    finally { if (mounted) setState(() => busy = false); }
  }
  @override Widget build(BuildContext context) => Scaffold(body: Center(child: SingleChildScrollView(padding: const EdgeInsets.all(28), child: ConstrainedBox(constraints: const BoxConstraints(maxWidth: 430), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [
    Icon(Icons.casino_outlined, size: 48, color: Theme.of(context).colorScheme.primary), const SizedBox(height: 28), const Text('千变', style: TextStyle(fontSize: 38, fontWeight: FontWeight.w700)),
    const SizedBox(height: 8), Text('每一次投掷，都有新的故事。', style: TextStyle(color: Theme.of(context).colorScheme.onSurfaceVariant)), const SizedBox(height: 40),
    if (!kIsWeb) ...[TextField(controller: address, decoration: const InputDecoration(labelText: '后台地址')), const SizedBox(height: 16)],
    TextField(controller: secret, obscureText: true, onSubmitted: (_) => submit(), decoration: const InputDecoration(labelText: '管理令牌', helperText: '保存在 data/config/admin-token.txt')), const SizedBox(height: 24),
    if (error != null) ...[Text(error!, style: TextStyle(color: Theme.of(context).colorScheme.error)), const SizedBox(height: 16)],
    SizedBox(width: double.infinity, child: FilledButton.icon(onPressed: busy ? null : submit, icon: busy ? const SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2)) : const Icon(Icons.arrow_forward), label: const Text('进入千变'))),
  ])))));
}

class Workspace extends StatefulWidget {
  const Workspace({super.key, required this.api, required this.themeMode, required this.onTheme, required this.onLogout});
  final Api api; final ThemeMode themeMode; final ValueChanged<ThemeMode> onTheme; final VoidCallback onLogout;
  @override State<Workspace> createState() => _WorkspaceState();
}
class _WorkspaceState extends State<Workspace> {
  int page = 0; bool expanded = true; bool busy = false; String? error;
  Map<String, dynamic> status = {}; List<dynamic> plugins = [], worlds = [], backups = [];
  StreamSubscription<Map<String, dynamic>>? events;
  Timer? retry;
  final pages = const [('概览', Icons.dashboard_outlined), ('账号与群', Icons.forum_outlined), ('角色卡', Icons.badge_outlined), ('规则', Icons.tune), ('牌堆', Icons.style_outlined), ('跑团记录', Icons.auto_stories_outlined), ('插件', Icons.extension_outlined), ('模拟聊天', Icons.terminal), ('设置', Icons.settings_outlined)];
  @override void initState() { super.initState(); refresh(); subscribe(); }
  @override void dispose() { events?.cancel(); retry?.cancel(); super.dispose(); }
  void subscribe() { events?.cancel(); events = widget.api.events().listen((event) { if (!mounted) return; setState(() { final notices = List<dynamic>.from(status['notices'] ?? []); notices.add(event); if (notices.length > 100) notices.removeAt(0); status['notices'] = notices; }); }, onError: (_) => reconnect(), onDone: reconnect); }
  void reconnect() { if (mounted) retry = Timer(const Duration(seconds: 5), () { refresh(); subscribe(); }); }
  Future<void> refresh() async { await action(() async { final data = await Future.wait([widget.api.request('/status'), widget.api.request('/plugins'), widget.api.request('/worlds'), widget.api.request('/backups')]); if (mounted) setState(() { status = Map<String, dynamic>.from(data[0]); plugins = data[1]; worlds = data[2]; backups = data[3]; }); }); }
  Future<void> action(Future<void> Function() run) async { if (mounted) setState(() { busy = true; error = null; }); try { await run(); } catch (e) { if (mounted) setState(() => error = '$e'); } finally { if (mounted) setState(() => busy = false); } }
  Future<void> message(String value) async { if (mounted) ScaffoldMessenger.of(context).showSnackBar(SnackBar(content: Text(value))); }
  Future<void> editJson(String title, dynamic initial, Future<void> Function(dynamic) save) async {
    final control = TextEditingController(text: const JsonEncoder.withIndent('  ').convert(initial)); String? problem; bool saving = false;
    await showDialog<void>(context: context, builder: (ctx) => StatefulBuilder(builder: (ctx, set) => AlertDialog(title: Text(title), content: SizedBox(width: 700, child: Column(mainAxisSize: MainAxisSize.min, children: [Flexible(child: TextField(controller: control, minLines: 8, maxLines: 22, style: const TextStyle(fontFamily: 'monospace', fontSize: 13), decoration: InputDecoration(errorText: problem))), const SizedBox(height: 8)])), actions: [TextButton(onPressed: saving ? null : () => Navigator.pop(ctx), child: const Text('取消')), FilledButton(onPressed: saving ? null : () async { set(() => saving = true); try { await save(jsonDecode(control.text)); if (ctx.mounted) Navigator.pop(ctx); } catch (e) { set(() => problem = '$e'); } finally { if (ctx.mounted) set(() => saving = false); } }, child: const Text('保存'))])));
    control.dispose();
  }
  Future<String?> prompt(String title, {String initial = '', String? hint}) async {
    final c = TextEditingController(text: initial); final result = await showDialog<String>(context: context, builder: (ctx) => AlertDialog(title: Text(title), content: TextField(controller: c, autofocus: true, decoration: InputDecoration(hintText: hint), onSubmitted: (v) => Navigator.pop(ctx, v)), actions: [TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('取消')), FilledButton(onPressed: () => Navigator.pop(ctx, c.text), child: const Text('确定'))])); c.dispose(); return result;
  }
  Future<void> configurePlugin(dynamic plugin) async {
    final id = plugin['manifest']['id'];
    final saved = await widget.api.request('/plugin-config/$id');
    final values = Map<String, dynamic>.from(saved['value'] ?? {});
    final props = Map<String, dynamic>.from(plugin['manifest']['config_schema']['properties'] ?? {});
    Future<void> save(dynamic value) async { await widget.api.request('/plugin-config/$id', method: 'PUT', body: {'value': value, 'revision': saved['revision']}); }
    if (props.isEmpty) { await editJson('插件配置 · $id', values, save); return; }
    if (!mounted) return;
    String? problem;
    await showDialog<void>(context: context, builder: (ctx) => StatefulBuilder(builder: (ctx, update) => AlertDialog(
      title: Text('配置 $id'),
      content: SizedBox(width: 520, child: SingleChildScrollView(child: Column(mainAxisSize: MainAxisSize.min, children: [
        for (final entry in props.entries) Padding(padding: const EdgeInsets.only(bottom: 14), child: Builder(builder: (ctx) {
          final field = entry.value as Map;
          final label = '${field['title'] ?? entry.key}';
          values.putIfAbsent(entry.key, () => field['default']);
          if (field['type'] == 'boolean') return SwitchListTile(title: Text(label), value: values[entry.key] == true, onChanged: (v) => update(() => values[entry.key] = v));
          if (field['enum'] is List) return DropdownButtonFormField<dynamic>(decoration: InputDecoration(labelText: label), initialValue: values[entry.key], items: (field['enum'] as List).map((v) => DropdownMenuItem<dynamic>(value: v, child: Text('$v'))).toList(), onChanged: (v) => values[entry.key] = v);
          return TextFormField(initialValue: '${values[entry.key] ?? ''}', decoration: InputDecoration(labelText: label, helperText: field['description'] as String?), keyboardType: field['type'] == 'integer' || field['type'] == 'number' ? TextInputType.number : TextInputType.text, onChanged: (v) => values[entry.key] = field['type'] == 'integer' ? int.tryParse(v) : field['type'] == 'number' ? double.tryParse(v) : v);
        })),
        if (problem != null) Text(problem!, style: TextStyle(color: Theme.of(ctx).colorScheme.error)),
      ]))), actions: [TextButton(onPressed: () => Navigator.pop(ctx), child: const Text('取消')), FilledButton(onPressed: () async { try { await save(values); if (ctx.mounted) Navigator.pop(ctx); } catch (e) { update(() => problem = '$e'); } }, child: const Text('保存'))],
    )));
  }
  Future<bool> confirm(String title, String detail) async => await showDialog<bool>(context: context, builder: (ctx) => AlertDialog(title: Text(title), content: Text(detail), actions: [TextButton(onPressed: () => Navigator.pop(ctx, false), child: const Text('取消')), FilledButton(onPressed: () => Navigator.pop(ctx, true), child: const Text('确定'))])) ?? false;
  @override Widget build(BuildContext context) {
    final wide = MediaQuery.sizeOf(context).width >= 840;
    return Scaffold(drawer: wide ? null : Drawer(child: navigation(true)), body: Row(children: [if (wide) SizedBox(width: expanded ? 205 : 76, child: navigation(expanded)), Expanded(child: Column(children: [
      Container(padding: const EdgeInsets.symmetric(horizontal: 24, vertical: 14), color: Theme.of(context).colorScheme.surfaceContainerLow, child: Row(children: [
        if (!wide) Builder(builder: (ctx) => IconButton(onPressed: () => Scaffold.of(ctx).openDrawer(), icon: const Icon(Icons.menu))),
        Icon(pages[page].$2, size: 22), const SizedBox(width: 12), Expanded(child: Text(pages[page].$1, style: Theme.of(context).textTheme.titleLarge)),
        if (busy) const Padding(padding: EdgeInsets.only(right: 12), child: SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2))),
        IconButton(tooltip: '刷新', onPressed: busy ? null : refresh, icon: const Icon(Icons.refresh)),
      ])),
      if (error != null) MaterialBanner(content: Text(error!), actions: [TextButton(onPressed: () => setState(() => error = null), child: const Text('关闭'))]),
      Expanded(child: IndexedStack(index: page, children: [overview(), accounts(), characters(), ContentPage(api: widget.api, kind: 'rules', edit: editJson, prompt: prompt), ContentPage(api: widget.api, kind: 'decks', edit: editJson, prompt: prompt), LogsPage(api: widget.api, worlds: worlds), pluginPage(), ChatPage(api: widget.api), settings()])),
    ]))]));
  }
  Widget navigation(bool labels) => SafeArea(child: Column(children: [
    Padding(padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 24), child: Row(children: [Icon(Icons.casino_outlined, color: Theme.of(context).colorScheme.primary, size: 30), if (labels) ...[const SizedBox(width: 12), const Expanded(child: Text('千变', style: TextStyle(fontSize: 24, fontWeight: FontWeight.w700)))]])),
    Expanded(child: ListView(padding: const EdgeInsets.symmetric(horizontal: 10), children: List.generate(pages.length, (i) => Padding(padding: const EdgeInsets.only(bottom: 5), child: Material(color: page == i ? Theme.of(context).colorScheme.secondaryContainer : Colors.transparent, borderRadius: BorderRadius.circular(9), child: Tooltip(message: pages[i].$1, child: ListTile(dense: true, shape: RoundedRectangleBorder(borderRadius: BorderRadius.circular(9)), leading: Icon(pages[i].$2, size: 21), title: labels ? Text(pages[i].$1) : null, onTap: () { setState(() => page = i); if (Scaffold.maybeOf(context)?.isDrawerOpen ?? false) Navigator.pop(context); }))))),
    if (labels) Padding(padding: const EdgeInsets.all(18), child: Text('千变 ${status['version'] ?? '0.1.0'}', style: Theme.of(context).textTheme.bodySmall)),
    IconButton(tooltip: labels ? '收起侧栏' : '展开侧栏', onPressed: () => setState(() => expanded = !expanded), icon: Icon(labels ? Icons.chevron_left : Icons.chevron_right)), const SizedBox(height: 12),
  ]));
  Widget body(List<Widget> children) => ListView(padding: const EdgeInsets.all(24), children: children);
  Widget section(String title, List<Widget> children, {Widget? action}) => Padding(padding: const EdgeInsets.only(bottom: 20), child: Card(child: Padding(padding: const EdgeInsets.all(22), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Row(children: [Expanded(child: Text(title, style: Theme.of(context).textTheme.titleMedium)), if (action != null) action]), const SizedBox(height: 16), ...children]))));
  Widget overview() { final connections = status['connections'] as List? ?? []; final notices = status['notices'] as List? ?? []; return body([
    Padding(padding: const EdgeInsets.only(bottom: 24), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Text('故事，从这里开始', style: Theme.of(context).textTheme.headlineMedium?.copyWith(fontWeight: FontWeight.w600)), const SizedBox(height: 8), Text(connections.isEmpty ? '连接一个机器人账号，或先在模拟聊天中试掷。' : '${connections.length} 个账号在线，千变正在运行。')])),
    Wrap(spacing: 16, runSpacing: 16, children: [('在线账号', '${connections.length}', Icons.forum_outlined), ('已启用插件', '${plugins.where((p) => p['enabled'] == true).length}', Icons.extension_outlined), ('运行时间', '${((status['uptime_seconds'] ?? 0) / 60).floor()} 分钟', Icons.schedule)].map((v) => SizedBox(width: 225, child: Card(child: Padding(padding: const EdgeInsets.all(22), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Icon(v.$3, color: Theme.of(context).colorScheme.primary), const SizedBox(height: 18), Text(v.$2, style: Theme.of(context).textTheme.headlineSmall), Text(v.$1)]))))).toList()), const SizedBox(height: 24),
    section('最近活动', notices.isEmpty ? [const Text('还没有活动。')] : notices.reversed.take(30).map<Widget>((n) => ListTile(contentPadding: EdgeInsets.zero, leading: Icon(n['level'] == 'warning' ? Icons.info_outline : Icons.circle, size: n['level'] == 'warning' ? 20 : 7), title: Text('${n['message']}'), subtitle: Text('${n['time']}'))).toList()),
  ]); }
  Widget accounts() => body([section('机器人连接', [const Text('通过 OneBot 11 连接 QQ 协议端。反向连接使用 Universal，必须配置独立访问令牌。'), const SizedBox(height: 16), FilledButton.tonalIcon(onPressed: () => action(() async { final cfg = await widget.api.request('/config'); await editJson('账号与群配置', cfg, (v) async { final r = await widget.api.request('/config', method: 'PUT', body: v); await message(r['message']); }); }), icon: const Icon(Icons.edit_outlined), label: const Text('编辑连接与权限')), const SizedBox(height: 12), ...((status['connections'] as List?) ?? []).map((id) => ListTile(leading: const Icon(Icons.check_circle_outline), title: Text('$id'), subtitle: const Text('已连接')))]), section('群规则与开关', [for (final w in worlds) for (final entry in ((w['value']['rooms'] as Map?) ?? {}).entries) ListTile(title: Text('${entry.key}'), subtitle: Text('${entry.value['rule']} · ${entry.value['enabled'] == true ? '启用' : '停用'}'), trailing: Switch(value: entry.value['enabled'] == true, onChanged: (v) => action(() async { entry.value['enabled'] = v; await widget.api.request('/world', method: 'PUT', body: {'scope': w['key'], 'revision': w['revision'], 'value': w['value']}); await refresh(); })))])]);
  Widget characters() { final cards = <Widget>[]; for (final w in worlds) { final players = (w['value']['players'] as Map?) ?? {}; for (final player in players.entries) { for (final entry in ((player.value['cards'] as Map?) ?? {}).entries) { cards.add(Card(child: ListTile(contentPadding: const EdgeInsets.all(20), leading: const Icon(Icons.person_outline), title: Text('${entry.key}'), subtitle: Text('${player.key} · ${entry.value['rule']}\n${(entry.value['attrs'] as Map).entries.take(5).map((e) => '${e.key} ${e.value}').join('  ')}'), isThreeLine: true, trailing: IconButton(tooltip: '编辑角色', icon: const Icon(Icons.edit_outlined), onPressed: () => editJson('编辑 ${entry.key}', entry.value, (v) async { player.value['cards'][entry.key] = v; await widget.api.request('/world', method: 'PUT', body: {'scope': w['key'], 'revision': w['revision'], 'value': w['value']}); await refresh(); })))); cards.add(const SizedBox(height: 12)); } } } return body([section('角色档案', [const Text('角色按账号与玩家保存，群绑定优先于默认角色。使用 .st new 名称 创建，.st lock 绑定当前群。')]), if (cards.isEmpty) const Padding(padding: EdgeInsets.all(32), child: Text('还没有角色卡。在群聊或模拟聊天中创建第一张。')), ...cards]); }
  Widget pluginPage() => body([section('扩展千变', [const Text('Python 与 Rust 插件按需启用。更新会等待正在执行的指令结束，失败时保留旧版本。'), const SizedBox(height: 16), FilledButton.tonalIcon(onPressed: () async { final path = await prompt('加载插件', hint: '插件ID/版本/plugin.json'); if (path == null) return; await action(() async { await widget.api.request('/plugins/load', method: 'POST', body: {'path': path}); await refresh(); }); }, icon: const Icon(Icons.add), label: const Text('加载或更新插件'))]), ...plugins.map((p) => Padding(padding: const EdgeInsets.only(bottom: 12), child: Card(child: Padding(padding: const EdgeInsets.all(20), child: Column(crossAxisAlignment: CrossAxisAlignment.start, children: [Row(children: [const Icon(Icons.extension_outlined), const SizedBox(width: 16), Expanded(child: Text('${p['manifest']['id']}', style: Theme.of(context).textTheme.titleMedium)), Text('${p['manifest']['version']}'), IconButton(tooltip: '插件配置', onPressed: () => action(() => configurePlugin(p)), icon: const Icon(Icons.tune)), const SizedBox(width: 16), Switch(value: p['enabled'] == true, onChanged: (v) => action(() async { await widget.api.request('/plugins/${p['manifest']['id']}/${v ? 'enable' : 'disable'}', method: 'POST'); await refresh(); }))]), const SizedBox(height: 8), Text((p['manifest']['commands'] as List).map((c) => '.$c').join('  ')), if (p['error'] != null) Text('${p['error']}', style: TextStyle(color: Theme.of(context).colorScheme.error))]))))]);
  Widget settings() => body([section('外观', [SegmentedButton<ThemeMode>(segments: const [ButtonSegment(value: ThemeMode.system, icon: Icon(Icons.brightness_auto), label: Text('跟随系统')), ButtonSegment(value: ThemeMode.light, icon: Icon(Icons.light_mode_outlined), label: Text('浅色')), ButtonSegment(value: ThemeMode.dark, icon: Icon(Icons.dark_mode_outlined), label: Text('深色'))], selected: {widget.themeMode}, onSelectionChanged: (s) => widget.onTheme(s.first))]), section('数据与备份', [SelectableText('${status['data_directory'] ?? ''}'), const SizedBox(height: 16), FilledButton.tonalIcon(onPressed: () => action(() async { final r = await widget.api.request('/backups', method: 'POST'); await message('备份已完成：${r['id']}'); await refresh(); }), icon: const Icon(Icons.save_outlined), label: const Text('创建完整备份')), const SizedBox(height: 16), ...backups.reversed.take(10).map((b) => ListTile(leading: const Icon(Icons.inventory_2_outlined), title: Text('$b'))), const Text('恢复需要先停止后台，再使用 restore 命令。恢复前会保存当前数据。')]), section('运行', [Text('版本 ${status['version'] ?? ''} · ${status['platform'] ?? ''}'), const SizedBox(height: 16), Wrap(spacing: 12, runSpacing: 12, children: [OutlinedButton(onPressed: () async { await widget.api.request('/logout', method: 'POST'); widget.api.token = ''; widget.onLogout(); }, child: const Text('退出登录')), OutlinedButton(onPressed: () async { if (await confirm('停止千变后台？', 'QQ 连接和插件会停止。关闭管理窗口本身不会停止后台。')) await action(() async { await widget.api.request('/shutdown', method: 'POST'); await message('后台正在停止'); }); }, child: const Text('停止后台'))])])]);
}

class ChatPage extends StatefulWidget {
  const ChatPage({super.key, required this.api}); final Api api;
  @override State<ChatPage> createState() => _ChatPageState();
}
class _ChatPageState extends State<ChatPage> {
  final input = TextEditingController(), user = TextEditingController(text: '调查员'), group = TextEditingController(text: '试验团');
  final List<(bool, String)> messages = []; bool busy = false;
  @override void dispose() { input.dispose(); user.dispose(); group.dispose(); super.dispose(); }
  Future<void> send() async { final text = input.text.trim(); if (text.isEmpty || busy) return; input.clear(); setState(() { busy = true; messages.add((true, text)); }); try { final result = await widget.api.request('/simulate', method: 'POST', body: {'context': {'platform': 'simulation', 'account': 'sandbox', 'user': user.text, 'group': group.text, 'name': user.text}, 'text': text}); if (mounted) setState(() => messages.add((false, '${result['public']}${result['private'] == null ? '' : '\n【仅自己可见】${result['private']}'}'))); } catch (e) { if (mounted) setState(() => messages.add((false, '$e'))); } finally { if (mounted) setState(() => busy = false); } }
  @override Widget build(BuildContext context) => Padding(padding: const EdgeInsets.all(24), child: Column(children: [Row(children: [Expanded(child: TextField(controller: user, decoration: const InputDecoration(labelText: '玩家'))), const SizedBox(width: 12), Expanded(child: TextField(controller: group, decoration: const InputDecoration(labelText: '会话')))]), const SizedBox(height: 12), const Text('模拟数据独立保存，不发送 QQ 消息。试试 .help 或 .r 1d100'), const SizedBox(height: 16), Expanded(child: ListView.builder(reverse: true, itemCount: messages.length, itemBuilder: (context, index) { final item = messages[messages.length - 1 - index]; return Align(alignment: item.$1 ? Alignment.centerRight : Alignment.centerLeft, child: Container(constraints: const BoxConstraints(maxWidth: 700), margin: const EdgeInsets.only(bottom: 12), padding: const EdgeInsets.all(16), decoration: BoxDecoration(color: item.$1 ? Theme.of(context).colorScheme.secondaryContainer : Theme.of(context).colorScheme.surfaceContainerLow, borderRadius: BorderRadius.circular(12)), child: SelectableText(item.$2))); })), const SizedBox(height: 12), Row(children: [Expanded(child: TextField(controller: input, onSubmitted: (_) => send(), decoration: const InputDecoration(hintText: '输入跑团指令…'))), const SizedBox(width: 12), IconButton.filled(onPressed: busy ? null : send, icon: busy ? const SizedBox.square(dimension: 18, child: CircularProgressIndicator(strokeWidth: 2)) : const Icon(Icons.arrow_upward))]) ]));
}

typedef JsonEditor = Future<void> Function(String, dynamic, Future<void> Function(dynamic));
typedef Prompt = Future<String?> Function(String, {String initial, String? hint});
class ContentPage extends StatefulWidget {
  const ContentPage({super.key, required this.api, required this.kind, required this.edit, required this.prompt}); final Api api; final String kind; final JsonEditor edit; final Prompt prompt;
  @override State<ContentPage> createState() => _ContentPageState();
}
class _ContentPageState extends State<ContentPage> {
  List<dynamic> names = []; String? error;
  @override void initState() { super.initState(); load(); }
  Future<void> load() async { try { final v = await widget.api.request('/content/${widget.kind}'); if (mounted) setState(() { names = v; error = null; }); } catch (e) { if (mounted) setState(() => error = '$e'); } }
  Future<void> edit(String name, {bool create = false}) async { try { final v = create ? (widget.kind == 'decks' ? {'without_replacement': false, 'entries': [{'text': '新的故事开始了。', 'weight': 1}]} : {'id': name.replaceAll('.json', ''), 'label': '自定义规则', 'faces': 100, 'comparison': 'lte', 'critical': 1, 'fumble': 100}) : await widget.api.request('/content/${widget.kind}/${Uri.encodeComponent(name)}'); await widget.edit(name, v, (value) async { await widget.api.request('/content/${widget.kind}/${Uri.encodeComponent(name)}', method: 'PUT', body: value); await load(); }); } catch (e) { if (mounted) setState(() => error = '$e'); } }
  @override Widget build(BuildContext context) => ListView(padding: const EdgeInsets.all(24), children: [Row(children: [Expanded(child: Text(widget.kind == 'decks' ? '自己的牌堆，随时生效。' : '内置 CoC7 与 DND5E；复杂规则可由插件扩展。')), FilledButton.tonalIcon(onPressed: () async { final name = await widget.prompt('新建${widget.kind == 'decks' ? '牌堆' : '规则'}', hint: '名称.json'); if (name == null || name.isEmpty) return; await edit(name.endsWith('.json') ? name : '$name.json', create: true); }, icon: const Icon(Icons.add), label: const Text('新建')), IconButton(onPressed: load, icon: const Icon(Icons.refresh))]), const SizedBox(height: 20), if (error != null) Text(error!, style: TextStyle(color: Theme.of(context).colorScheme.error)), ...names.map((name) => Padding(padding: const EdgeInsets.only(bottom: 10), child: Card(child: ListTile(leading: Icon(widget.kind == 'decks' ? Icons.style_outlined : Icons.tune), title: Text('$name'), trailing: const Icon(Icons.chevron_right), onTap: () => edit('$name'))))), if (names.isEmpty) const Padding(padding: EdgeInsets.all(40), child: Center(child: Text('还没有自定义内容。')))]);
}

class LogsPage extends StatefulWidget {
  const LogsPage({super.key, required this.api, required this.worlds}); final Api api; final List<dynamic> worlds;
  @override State<LogsPage> createState() => _LogsPageState();
}
class _LogsPageState extends State<LogsPage> {
  final scope = TextEditingController(), session = TextEditingController(); List<dynamic> rows = [], sessions = []; String? error;
  @override void initState() { super.initState(); loadSessions(); }
  Future<void> loadSessions() async { try { final value = await widget.api.request('/sessions'); if (mounted) setState(() => sessions = value); } catch (e) { if (mounted) setState(() => error = '$e'); } }
  @override void dispose() { scope.dispose(); session.dispose(); super.dispose(); }
  String query([String format = 'txt']) => Uri(queryParameters: {'scope': scope.text, 'session': session.text, 'format': format}).query;
  Future<void> load({bool more = false}) async { try { final result = await widget.api.request('/logs?${query()}&after=${more && rows.isNotEmpty ? rows.last['id'] : 0}'); if (mounted) setState(() { rows = more ? [...rows, ...result] : result; error = null; }); } catch (e) { if (mounted) setState(() => error = '$e'); } }
  @override Widget build(BuildContext context) => ListView(padding: const EdgeInsets.all(24), children: [Row(children: [Expanded(child: DropdownButtonFormField<int>(decoration: const InputDecoration(labelText: '历史团录'), items: List.generate(sessions.length, (i) => DropdownMenuItem(value: i, child: Text('${sessions[i]['session']} · ${sessions[i]['count']} 条'))), onChanged: (i) { if (i != null) { scope.text = '${sessions[i]['scope']}'; session.text = '${sessions[i]['session']}'; load(); } })), IconButton(onPressed: loadSessions, icon: const Icon(Icons.refresh))]), const SizedBox(height: 14), DropdownButtonFormField<String>(decoration: const InputDecoration(labelText: '账号数据'), items: widget.worlds.map<DropdownMenuItem<String>>((w) => DropdownMenuItem(value: '${w['key']}', child: Text('${w['key']}'))).toList(), onChanged: (v) => scope.text = v ?? ''), const SizedBox(height: 14), TextField(controller: session, decoration: const InputDecoration(labelText: '日志名称', helperText: '与 .log on 名称 保持一致')), const SizedBox(height: 16), Wrap(spacing: 12, runSpacing: 12, children: [FilledButton.tonal(onPressed: load, child: const Text('查看记录')), for (final format in ['txt', 'html', 'json']) OutlinedButton(onPressed: () async { try { final bytes = await widget.api.download('/export?${query(format)}'); final saved = await platform.save(bytes, 'qianbian-log.$format', format == 'html' ? 'text/html' : format == 'json' ? 'application/json' : 'text/plain'); if (context.mounted && saved) ScaffoldMessenger.of(context).showSnackBar(const SnackBar(content: Text('团录已导出。'))); } catch (e) { if (mounted) setState(() => error = '$e'); } }, child: Text('导出 ${format.toUpperCase()}'))]), const SizedBox(height: 20), if (error != null) Text(error!), ...rows.map((r) => ListTile(title: SelectableText('${r['text']}'), subtitle: Text('${r['actor']} · ${r['time']}'))), if (rows.isNotEmpty) TextButton(onPressed: () => load(more: true), child: const Text('加载更多'))]);
}
