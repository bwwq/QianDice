import 'dart:async';
import 'dart:convert';
import 'package:flutter/foundation.dart';
import 'package:flutter/services.dart' show Clipboard, ClipboardData;
import 'package:fluent_ui/fluent_ui.dart';
import 'package:http/http.dart' as http;
import 'appearance.dart';
import 'backup_settings.dart';
import 'platform_web.dart'
    if (dart.library.io) 'platform_native.dart'
    as platform;
import 'web_client_stub.dart'
    if (dart.library.js_interop) 'web_client.dart'
    as client;

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
  Future<Uint8List> download(String path) async {
    final response = await _client.get(
      uri(path),
      headers: {
        if (!kIsWeb && token.isNotEmpty) 'authorization': 'Bearer $token',
      },
    );
    if (response.statusCode != 200)
      throw Exception('导出失败 (${response.statusCode})');
    return response.bodyBytes;
  }

  Future<dynamic> request(
    String path, {
    String method = 'GET',
    Object? body,
  }) async {
    final request = http.Request(method, uri(path));
    request.headers.addAll({
      'content-type': 'application/json',
      'x-qianbian': '1',
      if (!kIsWeb && token.isNotEmpty) 'authorization': 'Bearer $token',
    });
    if (body != null) request.body = jsonEncode(body);
    final response = await http.Response.fromStream(
      await _client.send(request),
    );
    if (response.statusCode == 401) throw Exception('请登录或检查管理令牌');
    dynamic value;
    try {
      value = jsonDecode(response.body);
    } catch (_) {
      value = response.body;
    }
    if (response.statusCode >= 400)
      throw Exception(
        value is Map
            ? value['error'] ?? '请求失败'
            : '请求失败 (${response.statusCode})',
      );
    return value;
  }

  Stream<Map<String, dynamic>> events() async* {
    final request = http.Request('GET', uri('/events'));
    if (!kIsWeb && token.isNotEmpty)
      request.headers['authorization'] = 'Bearer $token';
    final response = await _client.send(request);
    if (response.statusCode != 200) return;
    await for (final line
        in response.stream
            .transform(utf8.decoder)
            .transform(const LineSplitter())) {
      if (line.startsWith('data:')) {
        try {
          yield jsonDecode(line.substring(5).trim()) as Map<String, dynamic>;
        } catch (_) {}
      }
    }
  }
}

IconData glyph(String name) =>
    FluentIcons.allIcons[name] ?? FluentIcons.settings;

String readableTime(dynamic value) {
  final date = DateTime.tryParse('$value')?.toLocal();
  if (date == null) return '$value';
  return '${date.month}/${date.day} ${date.hour.toString().padLeft(2, '0')}:${date.minute.toString().padLeft(2, '0')}';
}

Widget field(String label, Widget child, {String? hint}) => Padding(
  padding: const EdgeInsets.only(bottom: 16),
  child: InfoLabel(
    label: label,
    child: Column(
      crossAxisAlignment: CrossAxisAlignment.start,
      children: [
        child,
        if (hint != null)
          Padding(
            padding: const EdgeInsets.only(top: 6),
            child: Text(hint, style: const TextStyle(fontSize: 11)),
          ),
      ],
    ),
  ),
);

Widget commandButton(
  String label,
  VoidCallback? onPressed, {
  String? icon,
  bool primary = false,
}) {
  final content = Row(
    mainAxisSize: MainAxisSize.min,
    children: [
      if (icon != null) ...[
        Icon(glyph(icon), size: 16),
        const SizedBox(width: 8),
      ],
      Text(label),
    ],
  );
  return primary
      ? FilledButton(onPressed: onPressed, child: content)
      : Button(onPressed: onPressed, child: content);
}

class QianbianApp extends StatefulWidget {
  const QianbianApp({super.key, required this.api});
  final Api api;
  @override
  State<QianbianApp> createState() => _QianbianAppState();
}

class _QianbianAppState extends State<QianbianApp> {
  ThemeMode mode = ThemeMode.system;
  bool loggedIn = false;
  @override
  void initState() {
    super.initState();
    _resume();
  }

  Future<void> _resume() async {
    try {
      final status = await widget.api.request('/status');
      if (status['api'] != 1) throw Exception('管理接口版本不兼容');
      if (mounted) setState(() => loggedIn = true);
    } catch (_) {}
  }

  @override
  Widget build(BuildContext context) => FluentApp(
    title: '千变',
    debugShowCheckedModeBanner: false,
    theme: workspaceTheme(Brightness.light),
    darkTheme: workspaceTheme(Brightness.dark),
    themeMode: mode,
    home: loggedIn
        ? Workspace(
            api: widget.api,
            themeMode: mode,
            onTheme: (value) => setState(() => mode = value),
            onLogout: () => setState(() => loggedIn = false),
          )
        : Login(
            api: widget.api,
            onLogin: () => setState(() => loggedIn = true),
          ),
  );
}

class Login extends StatefulWidget {
  const Login({super.key, required this.api, required this.onLogin});
  final Api api;
  final VoidCallback onLogin;
  @override
  State<Login> createState() => _LoginState();
}

class _LoginState extends State<Login> {
  late final address = TextEditingController(text: widget.api.base);
  final secret = TextEditingController();
  bool busy = false;
  String? error;
  @override
  void dispose() {
    address.dispose();
    secret.dispose();
    super.dispose();
  }

  Future<void> submit() async {
    if (busy) return;
    setState(() {
      busy = true;
      error = null;
    });
    try {
      widget.api.base = address.text.trim().replaceAll(RegExp(r'/$'), '');
      widget.api.token = secret.text.trim();
      await widget.api.request(
        '/login',
        method: 'POST',
        body: {'token': widget.api.token},
      );
      final status = await widget.api.request('/status');
      if (status['api'] != 1) throw Exception('管理端与后台版本不兼容');
      widget.onLogin();
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  @override
  Widget build(BuildContext context) => ScaffoldPage(
    content: Center(
      child: SingleChildScrollView(
        padding: const EdgeInsets.all(28),
        child: SizedBox(
          width: 420,
          child: Surface(
            padding: const EdgeInsets.all(32),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                const DiceMark(size: 52),
                const SizedBox(height: 20),
                Text('千变', style: FluentTheme.of(context).typography.title),
                const SizedBox(height: 28),
                if (!kIsWeb) field('后台地址', TextBox(controller: address)),
                field(
                  '管理令牌',
                  PasswordBox(controller: secret, onSubmitted: (_) => submit()),
                  hint: 'data/config/admin-token.txt',
                ),
                if (error != null) ...[
                  InfoBar(
                    title: const Text('无法登录'),
                    content: Text(error!),
                    severity: InfoBarSeverity.error,
                  ),
                  const SizedBox(height: 16),
                ],
                SizedBox(
                  width: double.infinity,
                  child: FilledButton(
                    onPressed: busy ? null : submit,
                    child: busy
                        ? const SizedBox.square(
                            dimension: 18,
                            child: ProgressRing(strokeWidth: 2),
                          )
                        : const Text('登录'),
                  ),
                ),
              ],
            ),
          ),
        ),
      ),
    ),
  );
}

class Workspace extends StatefulWidget {
  const Workspace({
    super.key,
    required this.api,
    required this.themeMode,
    required this.onTheme,
    required this.onLogout,
  });
  final Api api;
  final ThemeMode themeMode;
  final ValueChanged<ThemeMode> onTheme;
  final VoidCallback onLogout;
  @override
  State<Workspace> createState() => _WorkspaceState();
}

class _WorkspaceState extends State<Workspace> {
  int page = 0;
  bool expanded = true, busy = false;
  String? error, feedback;
  Map<String, dynamic> status = {};
  List<dynamic> plugins = [], worlds = [], backups = [];
  StreamSubscription<Map<String, dynamic>>? events;
  Timer? retry;
  final pageContentKey = GlobalKey();
  final pages = [
    ('概览', glyph('home')),
    ('账号与群', glyph('group')),
    ('角色卡', glyph('contact')),
    ('规则', glyph('library')),
    ('牌堆', glyph('album')),
    ('跑团记录', glyph('reading_mode')),
    ('插件', glyph('plug_connected')),
    ('模拟聊天', glyph('chat')),
    ('设置', FluentIcons.settings),
  ];
  @override
  void initState() {
    super.initState();
    refresh();
    subscribe();
  }

  @override
  void dispose() {
    events?.cancel();
    retry?.cancel();
    super.dispose();
  }

  void subscribe() {
    retry?.cancel();
    events?.cancel();
    events = widget.api.events().listen(
      (event) {
        if (!mounted) return;
        setState(() {
          final notices = List<dynamic>.from(status['notices'] ?? []);
          notices.add(event);
          if (notices.length > 100) notices.removeAt(0);
          status['notices'] = notices;
        });
      },
      onError: (_) => reconnect(),
      onDone: reconnect,
    );
  }

  void reconnect() {
    retry?.cancel();
    if (mounted)
      retry = Timer(const Duration(seconds: 5), () {
        refresh();
        subscribe();
      });
  }

  Future<void> refresh() async => action(() async {
    final values = await Future.wait([
      widget.api.request('/status'),
      widget.api.request('/plugins'),
      widget.api.request('/worlds'),
      widget.api.request('/backups'),
    ]);
    if (mounted)
      setState(() {
        status = Map<String, dynamic>.from(values[0]);
        plugins = values[1];
        worlds = values[2];
        backups = values[3];
      });
  });
  Future<void> action(Future<void> Function() run) async {
    if (mounted)
      setState(() {
        busy = true;
        error = null;
      });
    try {
      await run();
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    } finally {
      if (mounted) setState(() => busy = false);
    }
  }

  Future<void> message(String value) async {
    if (mounted) setState(() => feedback = value);
  }

  Future<void> editJson(
    String title,
    dynamic initial,
    Future<void> Function(dynamic) save,
  ) async {
    final control = TextEditingController(
      text: const JsonEncoder.withIndent('  ').convert(initial),
    );
    String? problem;
    bool saving = false;
    await showDialog<void>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, set) => ContentDialog(
          constraints: const BoxConstraints(maxWidth: 740),
          title: Text(title),
          content: Column(
            mainAxisSize: MainAxisSize.min,
            children: [
              SizedBox(
                height: (MediaQuery.sizeOf(ctx).height * .45)
                    .clamp(120, 350)
                    .toDouble(),
                child: TextBox(
                  controller: control,
                  expands: true,
                  maxLines: null,
                  style: const TextStyle(fontFamily: 'monospace', fontSize: 13),
                ),
              ),
              if (problem != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: InfoBar(
                    title: const Text('无法保存'),
                    content: Text(problem!),
                    severity: InfoBarSeverity.error,
                  ),
                ),
            ],
          ),
          actions: [
            Button(
              onPressed: saving ? null : () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: saving
                  ? null
                  : () async {
                      set(() => saving = true);
                      try {
                        await save(jsonDecode(control.text));
                        if (ctx.mounted) Navigator.pop(ctx);
                      } catch (e) {
                        if (ctx.mounted) set(() => problem = '$e');
                      } finally {
                        if (ctx.mounted) set(() => saving = false);
                      }
                    },
              child: Text(saving ? '保存中…' : '保存'),
            ),
          ],
        ),
      ),
    );
    control.dispose();
  }

  Future<String?> prompt(
    String title, {
    String initial = '',
    String? hint,
  }) async {
    final control = TextEditingController(text: initial);
    final result = await showDialog<String>(
      context: context,
      builder: (ctx) => ContentDialog(
        title: Text(title),
        content: TextBox(
          controller: control,
          autofocus: true,
          placeholder: hint,
          onSubmitted: (value) => Navigator.pop(ctx, value),
        ),
        actions: [
          Button(onPressed: () => Navigator.pop(ctx), child: const Text('取消')),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, control.text),
            child: const Text('确定'),
          ),
        ],
      ),
    );
    control.dispose();
    return result;
  }

  Future<void> configurePlugin(dynamic plugin) async {
    final id = plugin['manifest']['id'];
    final saved = await widget.api.request('/plugin-config/$id');
    final values = Map<String, dynamic>.from(saved['value'] ?? {});
    final props = Map<String, dynamic>.from(
      plugin['manifest']['config_schema']['properties'] ?? {},
    );
    Future<void> save(dynamic value) async {
      await widget.api.request(
        '/plugin-config/$id',
        method: 'PUT',
        body: {'value': value, 'revision': saved['revision']},
      );
    }

    if (props.isEmpty) {
      await editJson('插件配置 · $id', values, save);
      return;
    }
    if (!mounted) return;
    final controls = <String, TextEditingController>{};
    for (final entry in props.entries) {
      values.putIfAbsent(entry.key, () => entry.value['default']);
      if (entry.value['enum'] is List &&
          values[entry.key] == null &&
          (entry.value['enum'] as List).isNotEmpty)
        values[entry.key] = entry.value['enum'][0];
      if (entry.value['type'] != 'boolean' && entry.value['enum'] is! List)
        controls[entry.key] = TextEditingController(
          text: '${values[entry.key] ?? ''}',
        );
    }
    String? problem;
    bool saving = false;
    await showDialog<void>(
      context: context,
      builder: (ctx) => StatefulBuilder(
        builder: (ctx, update) => ContentDialog(
          constraints: const BoxConstraints(maxWidth: 560),
          title: Text('配置 $id'),
          content: SingleChildScrollView(
            child: Column(
              mainAxisSize: MainAxisSize.min,
              children: [
                for (final entry in props.entries)
                  Builder(
                    builder: (ctx) {
                      final spec = entry.value as Map;
                      final label = '${spec['title'] ?? entry.key}';
                      if (spec['type'] == 'boolean')
                        return Padding(
                          padding: const EdgeInsets.only(bottom: 16),
                          child: ToggleSwitch(
                            content: Text(label),
                            checked: values[entry.key] == true,
                            onChanged: (value) =>
                                update(() => values[entry.key] = value),
                          ),
                        );
                      if (spec['enum'] is List)
                        return field(
                          label,
                          ComboBox<dynamic>(
                            value: values[entry.key],
                            items: (spec['enum'] as List)
                                .map(
                                  (value) => ComboBoxItem<dynamic>(
                                    value: value,
                                    child: Text('$value'),
                                  ),
                                )
                                .toList(),
                            onChanged: (value) =>
                                update(() => values[entry.key] = value),
                          ),
                        );
                      return field(
                        label,
                        TextBox(
                          controller: controls[entry.key],
                          keyboardType:
                              ['integer', 'number'].contains(spec['type'])
                              ? TextInputType.number
                              : TextInputType.text,
                          onChanged: (value) =>
                              values[entry.key] = spec['type'] == 'integer'
                              ? int.tryParse(value)
                              : spec['type'] == 'number'
                              ? double.tryParse(value)
                              : value,
                        ),
                        hint: spec['description'] as String?,
                      );
                    },
                  ),
                if (problem != null)
                  InfoBar(
                    title: const Text('无法保存'),
                    content: Text(problem!),
                    severity: InfoBarSeverity.error,
                  ),
              ],
            ),
          ),
          actions: [
            Button(
              onPressed: saving ? null : () => Navigator.pop(ctx),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: saving
                  ? null
                  : () async {
                      update(() => saving = true);
                      try {
                        await save(values);
                        if (ctx.mounted) Navigator.pop(ctx);
                      } catch (e) {
                        if (ctx.mounted) update(() => problem = '$e');
                      } finally {
                        if (ctx.mounted) update(() => saving = false);
                      }
                    },
              child: Text(saving ? '保存中…' : '保存'),
            ),
          ],
        ),
      ),
    );
    for (final control in controls.values) {
      control.dispose();
    }
  }

  Future<bool> confirm(String title, String detail) async =>
      await showDialog<bool>(
        context: context,
        builder: (ctx) => ContentDialog(
          title: Text(title),
          content: Text(detail),
          actions: [
            Button(
              onPressed: () => Navigator.pop(ctx, false),
              child: const Text('取消'),
            ),
            FilledButton(
              onPressed: () => Navigator.pop(ctx, true),
              child: const Text('确定'),
            ),
          ],
        ),
      ) ??
      false;

  @override
  Widget build(BuildContext context) => NavigationView(
    content: DesktopShell(
      destinations: pages,
      selected: page,
      onSelect: (value) => setState(() => page = value),
      expanded: expanded,
      onToggle: () => setState(() => expanded = !expanded),
      onTheme: () => widget.onTheme(
        FluentTheme.of(context).brightness == Brightness.dark
            ? ThemeMode.light
            : ThemeMode.dark,
      ),
      onRefresh: refresh,
      busy: busy,
      child: Align(
        alignment: Alignment.topCenter,
        child: ConstrainedBox(
          constraints: const BoxConstraints(maxWidth: 1120),
          child: Column(
            children: [
              if (error != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
                  child: InfoBar(
                    title: const Text('操作未完成'),
                    content: Text(error!),
                    severity: InfoBarSeverity.error,
                    onClose: () => setState(() => error = null),
                  ),
                ),
              if (feedback != null)
                Padding(
                  padding: const EdgeInsets.fromLTRB(20, 0, 20, 12),
                  child: InfoBar(
                    title: Text(feedback!),
                    severity: InfoBarSeverity.success,
                    onClose: () => setState(() => feedback = null),
                  ),
                ),
              Expanded(
                child: IndexedStack(
                  key: pageContentKey,
                  index: page,
                  children: [
                    overview(),
                    accounts(),
                    characters(),
                    ContentPage(
                      api: widget.api,
                      kind: 'rules',
                      edit: editJson,
                      prompt: prompt,
                    ),
                    ContentPage(
                      api: widget.api,
                      kind: 'decks',
                      edit: editJson,
                      prompt: prompt,
                    ),
                    LogsPage(api: widget.api, worlds: worlds),
                    pluginPage(),
                    ChatPage(api: widget.api),
                    settings(),
                  ],
                ),
              ),
            ],
          ),
        ),
      ),
    ),
  );
  Widget body(List<Widget> children) => ListView(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    children: children,
  );
  Widget section(String title, List<Widget> children, {Widget? action}) =>
      Padding(
        padding: const EdgeInsets.only(bottom: 20),
        child: Surface(
          padding: const EdgeInsets.all(22),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Expanded(
                    child: Text(
                      title,
                      style: FluentTheme.of(context).typography.subtitle,
                    ),
                  ),
                  if (action != null) action,
                ],
              ),
              const SizedBox(height: 18),
              ...children,
            ],
          ),
        ),
      );
  Widget overview() {
    final colors = WorkspacePalette.of(context);
    final connections = status['connections'] as List? ?? [];
    final notices = status['notices'] as List? ?? [];
    final cardCount = worlds.fold<int>(
      0,
      (sum, world) =>
          sum +
          ((world['value']['players'] as Map?) ?? {}).values.fold<int>(
            0,
            (count, player) => count + ((player['cards'] as Map?) ?? {}).length,
          ),
    );
    final metrics = [
      ('在线账号', '${connections.length}', 'group'),
      ('角色卡', '$cardCount', 'contact'),
      (
        '启用插件',
        '${plugins.where((plugin) => plugin['enabled'] == true).length}',
        'plug_connected',
      ),
      ('运行时间', '${((status['uptime_seconds'] ?? 0) / 60).floor()} 分钟', 'clock'),
    ];
    Widget metric((String, String, String) value, double width) => SizedBox(
      width: width,
      child: Row(
        children: [
          Container(
            width: 42,
            height: 42,
            decoration: BoxDecoration(
              color: colors.selected,
              borderRadius: BorderRadius.circular(13),
            ),
            child: Icon(glyph(value.$3), size: 18, color: colors.accent),
          ),
          const SizedBox(width: 14),
          Expanded(
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Text(
                  value.$1,
                  style: TextStyle(fontSize: 12, color: colors.muted),
                ),
                const SizedBox(height: 5),
                Text(
                  value.$2,
                  style: TextStyle(
                    fontSize: 25,
                    fontWeight: FontWeight.w600,
                    color: colors.text,
                  ),
                ),
              ],
            ),
          ),
        ],
      ),
    );
    final accountPanel = Surface(
      padding: const EdgeInsets.all(24),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Row(
            children: [
              Text(
                '账号连接',
                style: TextStyle(
                  fontSize: 17,
                  fontWeight: FontWeight.w600,
                  color: colors.text,
                ),
              ),
              const Spacer(),
              Text(
                'OneBot 11',
                style: TextStyle(fontSize: 11, color: colors.muted),
              ),
            ],
          ),
          const SizedBox(height: 24),
          if (connections.isEmpty) ...[
            Row(
              children: [
                const DiceMark(size: 58),
                const SizedBox(width: 18),
                Expanded(
                  child: Column(
                    crossAxisAlignment: CrossAxisAlignment.start,
                    children: [
                      Text(
                        '连接你的骰子',
                        style: TextStyle(
                          fontSize: 20,
                          fontWeight: FontWeight.w600,
                          color: colors.text,
                        ),
                      ),
                      const SizedBox(height: 8),
                      Text(
                        '添加 QQ 账号，开始下一场跑团。',
                        style: TextStyle(fontSize: 13, color: colors.muted),
                      ),
                    ],
                  ),
                ),
              ],
            ),
            const SizedBox(height: 28),
            commandButton(
              '添加账号',
              () => setState(() => page = 1),
              icon: 'add',
              primary: true,
            ),
          ] else ...[
            for (final id in connections)
              ListTile(
                leading: Icon(glyph('check_mark'), color: colors.success),
                title: Text('$id'),
                trailing: const StatusPill(label: '已连接'),
              ),
            const SizedBox(height: 16),
            commandButton(
              '管理账号',
              () => setState(() => page = 1),
              icon: 'group',
            ),
          ],
        ],
      ),
    );
    final quickPanel = Surface(
      padding: const EdgeInsets.all(20),
      child: Column(
        crossAxisAlignment: CrossAxisAlignment.start,
        children: [
          Padding(
            padding: const EdgeInsets.only(left: 4, bottom: 14),
            child: Text(
              '常用',
              style: TextStyle(
                fontSize: 17,
                fontWeight: FontWeight.w600,
                color: colors.text,
              ),
            ),
          ),
          for (final entry in [
            ('模拟掷骰', 'game', 7),
            ('角色卡', 'contact', 2),
            ('跑团记录', 'reading_mode', 5),
          ])
            Padding(
              padding: const EdgeInsets.only(bottom: 8),
              child: Button(
                onPressed: () => setState(() => page = entry.$3),
                style: ButtonStyle(
                  backgroundColor: const WidgetStatePropertyAll(
                    Colors.transparent,
                  ),
                  padding: const WidgetStatePropertyAll(
                    EdgeInsets.symmetric(horizontal: 4, vertical: 12),
                  ),
                ),
                child: Row(
                  children: [
                    Icon(glyph(entry.$2), size: 17, color: colors.accent),
                    const SizedBox(width: 12),
                    Expanded(child: Text(entry.$1, textAlign: TextAlign.start)),
                    Icon(glyph('chevron_right'), size: 10, color: colors.muted),
                  ],
                ),
              ),
            ),
        ],
      ),
    );
    return body([
      Surface(
        padding: const EdgeInsets.all(24),
        child: LayoutBuilder(
          builder: (context, constraints) {
            final columns = constraints.maxWidth >= 720 ? 4 : 2;
            final width = (constraints.maxWidth - (columns - 1) * 16) / columns;
            return Wrap(
              spacing: 16,
              runSpacing: 24,
              children: [for (final value in metrics) metric(value, width)],
            );
          },
        ),
      ),
      const SizedBox(height: 20),
      LayoutBuilder(
        builder: (context, constraints) => constraints.maxWidth >= 720
            ? Row(
                crossAxisAlignment: CrossAxisAlignment.start,
                children: [
                  Expanded(flex: 2, child: accountPanel),
                  const SizedBox(width: 20),
                  Expanded(child: quickPanel),
                ],
              )
            : Column(
                children: [
                  accountPanel,
                  const SizedBox(height: 20),
                  quickPanel,
                ],
              ),
      ),
      const SizedBox(height: 24),
      Padding(
        padding: const EdgeInsets.symmetric(horizontal: 4),
        child: Row(
          children: [
            Text(
              '最近动态',
              style: TextStyle(
                fontSize: 17,
                fontWeight: FontWeight.w600,
                color: colors.text,
              ),
            ),
            const Spacer(),
            Text(
              '${notices.length} 条',
              style: TextStyle(fontSize: 12, color: colors.muted),
            ),
          ],
        ),
      ),
      const SizedBox(height: 14),
      if (notices.isEmpty)
        Padding(
          padding: const EdgeInsets.symmetric(horizontal: 4, vertical: 20),
          child: Row(
            children: [
              Icon(glyph('info'), size: 16, color: colors.muted),
              const SizedBox(width: 10),
              Text('暂无动态', style: TextStyle(color: colors.muted)),
            ],
          ),
        )
      else
        Surface(
          child: Column(
            children: notices.reversed
                .take(30)
                .map<Widget>(
                  (notice) => ListTile(
                    leading: Icon(
                      glyph(
                        notice['level'] == 'warning' ? 'info' : 'circle_fill',
                      ),
                      size: 12,
                      color: colors.accent,
                    ),
                    title: Text('${notice['message']}'),
                    subtitle: Text(readableTime(notice['time'])),
                  ),
                )
                .toList(),
          ),
        ),
    ]);
  }

  Widget accounts() => body([
    section('机器人连接', [
      commandButton(
        '编辑账号',
        () => action(() async {
          final config = await widget.api.request('/config');
          await editJson('账号与群配置', config, (value) async {
            final result = await widget.api.request(
              '/config',
              method: 'PUT',
              body: value,
            );
            await message(result['message']);
          });
        }),
        icon: 'edit',
        primary: true,
      ),
      const SizedBox(height: 12),
      const Text(
        'OneBot 11 · 反向连接使用 Universal，连接配置需重启后台。',
        style: TextStyle(fontSize: 11),
      ),
      for (final id in (status['connections'] as List?) ?? [])
        ListTile(
          leading: Icon(glyph('check_mark')),
          title: Text('$id'),
          subtitle: const Text('已连接'),
        ),
    ]),
    section('会话', [
      for (final world in worlds)
        for (final entry in ((world['value']['rooms'] as Map?) ?? {}).entries)
          ListTile(
            title: Text('${entry.key}'),
            subtitle: Text('${entry.value['rule']}'),
            trailing: ToggleSwitch(
              checked: entry.value['enabled'] == true,
              onChanged: (value) => action(() async {
                final current = await widget.api.request(
                  '/world?scope=${Uri.encodeComponent('${world['key']}')}',
                );
                current['value']['rooms'][entry.key]['enabled'] = value;
                await widget.api.request(
                  '/world',
                  method: 'PUT',
                  body: {
                    'scope': world['key'],
                    'revision': current['revision'],
                    'value': current['value'],
                  },
                );
                await refresh();
              }),
            ),
          ),
      if (worlds.every(
        (world) => ((world['value']['rooms'] as Map?) ?? {}).isEmpty,
      ))
        const Text('暂无会话'),
    ]),
  ]);
  Widget characters() {
    final cards = <Widget>[];
    for (final world in worlds)
      for (final player
          in ((world['value']['players'] as Map?) ?? {}).entries) {
        for (final entry in ((player.value['cards'] as Map?) ?? {}).entries) {
          cards.add(
            Padding(
              padding: const EdgeInsets.only(bottom: 12),
              child: Surface(
                child: ListTile(
                  leading: Icon(glyph('contact')),
                  title: Text('${entry.key}'),
                  subtitle: Text(
                    '${player.key} · ${entry.value['rule']}\n${(entry.value['attrs'] as Map).entries.take(5).map((attr) => '${attr.key} ${attr.value}').join('  ')}',
                  ),
                  trailing: Tooltip(
                    message: '编辑角色',
                    child: IconButton(
                      icon: Icon(glyph('edit'), size: 16),
                      onPressed: () => editJson(
                        '编辑 ${entry.key}',
                        entry.value,
                        (value) async {
                          player.value['cards'][entry.key] = value;
                          await widget.api.request(
                            '/world',
                            method: 'PUT',
                            body: {
                              'scope': world['key'],
                              'revision': world['revision'],
                              'value': world['value'],
                            },
                          );
                          await refresh();
                        },
                      ),
                    ),
                  ),
                ),
              ),
            ),
          );
        }
      }
    return body([
      if (cards.isEmpty)
        section('暂无角色', [const Text('.st new 名称 创建角色 · .st lock 绑定当前群')]),
      ...cards,
    ]);
  }

  Widget pluginPage() => body([
    section('插件', [
      commandButton(
        '加载或更新',
        () async {
          final path = await prompt('加载插件', hint: '插件ID/版本/plugin.json');
          if (path == null || path.isEmpty) return;
          await action(() async {
            await widget.api.request(
              '/plugins/load',
              method: 'POST',
              body: {'path': path},
            );
            await refresh();
          });
        },
        icon: 'add',
        primary: true,
      ),
    ]),
    ...plugins.map(
      (plugin) => Padding(
        padding: const EdgeInsets.only(bottom: 12),
        child: Surface(
          padding: const EdgeInsets.all(20),
          child: Column(
            crossAxisAlignment: CrossAxisAlignment.start,
            children: [
              Row(
                children: [
                  Icon(glyph('plug_connected'), size: 20),
                  const SizedBox(width: 14),
                  Expanded(
                    child: Text(
                      '${plugin['manifest']['id']}',
                      style: FluentTheme.of(context).typography.subtitle,
                    ),
                  ),
                  Text('v${plugin['manifest']['version']}'),
                  const SizedBox(width: 12),
                  Tooltip(
                    message: '配置',
                    child: IconButton(
                      onPressed: () => action(() => configurePlugin(plugin)),
                      icon: Icon(glyph('settings'), size: 16),
                    ),
                  ),
                  const SizedBox(width: 12),
                  ToggleSwitch(
                    checked: plugin['enabled'] == true,
                    onChanged: (value) => action(() async {
                      await widget.api.request(
                        '/plugins/${plugin['manifest']['id']}/${value ? 'enable' : 'disable'}',
                        method: 'POST',
                      );
                      await refresh();
                    }),
                  ),
                ],
              ),
              const SizedBox(height: 12),
              Text(
                (plugin['manifest']['commands'] as List)
                    .map((command) => '.$command')
                    .join('  '),
                style: FluentTheme.of(context).typography.caption,
              ),
              if (plugin['error'] != null)
                Padding(
                  padding: const EdgeInsets.only(top: 12),
                  child: InfoBar(
                    title: const Text('插件异常'),
                    content: Text('${plugin['error']}'),
                    severity: InfoBarSeverity.error,
                  ),
                ),
            ],
          ),
        ),
      ),
    ),
  ]);
  Widget settings() {
    final colors = WorkspacePalette.of(context);
    final directory = '${status['data_directory'] ?? ''}';
    Widget group(String title, List<Widget> children, {Widget? action}) =>
        Padding(
          padding: const EdgeInsets.only(bottom: 16),
          child: Surface(
            padding: const EdgeInsets.all(20),
            child: Column(
              crossAxisAlignment: CrossAxisAlignment.start,
              children: [
                Row(
                  children: [
                    Expanded(
                      child: Text(
                        title,
                        style: TextStyle(
                          fontSize: 15,
                          fontWeight: FontWeight.w600,
                          color: colors.text,
                        ),
                      ),
                    ),
                    if (action != null) action,
                  ],
                ),
                const SizedBox(height: 14),
                ...children,
              ],
            ),
          ),
        );
    final themePicker = SizedBox(
      width: 200,
      child: Container(
        padding: const EdgeInsets.symmetric(horizontal: 10, vertical: 4),
        decoration: BoxDecoration(
          color: colors.control,
          borderRadius: BorderRadius.circular(10),
        ),
        child: ComboBox<ThemeMode>(
          isExpanded: true,
          value: widget.themeMode,
          onChanged: (mode) {
            if (mode != null) widget.onTheme(mode);
          },
          items: const [
            ComboBoxItem(value: ThemeMode.system, child: Text('跟随系统')),
            ComboBoxItem(value: ThemeMode.light, child: Text('浅色')),
            ComboBoxItem(value: ThemeMode.dark, child: Text('深色')),
          ],
        ),
      ),
    );
    return Align(
      alignment: Alignment.topLeft,
      child: ConstrainedBox(
        constraints: const BoxConstraints(maxWidth: 860),
        child: ListView(
          padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
          children: [
            group('外观', [
              LayoutBuilder(
                builder: (context, constraints) => constraints.maxWidth < 400
                    ? Column(
                        crossAxisAlignment: CrossAxisAlignment.start,
                        children: [
                          const Text('主题'),
                          const SizedBox(height: 10),
                          themePicker,
                        ],
                      )
                    : Row(
                        children: [
                          const Expanded(child: Text('主题')),
                          themePicker,
                        ],
                      ),
              ),
            ]),
            group('数据与备份', [
              Text('数据目录', style: TextStyle(fontSize: 12, color: colors.muted)),
              const SizedBox(height: 8),
              Container(
                padding: const EdgeInsets.fromLTRB(12, 8, 4, 8),
                decoration: BoxDecoration(
                  color: colors.control.withValues(alpha: .65),
                  borderRadius: BorderRadius.circular(10),
                ),
                child: Row(
                  children: [
                    Expanded(
                      child: SelectableText(
                        directory,
                        style: TextStyle(
                          fontSize: 13,
                          height: 1.5,
                          color: colors.text,
                        ),
                      ),
                    ),
                    const SizedBox(width: 8),
                    Tooltip(
                      message: '复制路径',
                      child: IconButton(
                        icon: const Icon(FluentIcons.copy, size: 15),
                        onPressed: directory.isEmpty
                            ? null
                            : () => action(() async {
                                await Clipboard.setData(
                                  ClipboardData(text: directory),
                                );
                                await message('路径已复制');
                              }),
                      ),
                    ),
                  ],
                ),
              ),
              const SizedBox(height: 20),
              BackupSettings(
                request: widget.api.request,
                refresh: refresh,
                notify: message,
              ),
              if (backups.isNotEmpty) ...[
                const SizedBox(height: 12),
                for (final backup in backups.reversed.take(10))
                  ListTile(
                    leading: Icon(glyph('archive'), size: 16),
                    title: Text('$backup'),
                  ),
              ],
              const SizedBox(height: 14),
              Text(
                '恢复前停止后台。自定义备份仅恢复所选内容，并另存当前数据。',
                style: TextStyle(
                  fontSize: 12,
                  height: 1.5,
                  color: colors.muted,
                ),
              ),
            ]),
            group('运行', [
              Text(
                '千变 ${status['version'] ?? ''} · ${status['platform'] ?? ''}',
                style: TextStyle(fontSize: 13, color: colors.muted),
              ),
              const SizedBox(height: 14),
              Wrap(
                spacing: 10,
                runSpacing: 10,
                children: [
                  commandButton('退出登录', () async {
                    await widget.api.request('/logout', method: 'POST');
                    widget.api.token = '';
                    widget.onLogout();
                  }, icon: 'sign_out'),
                  Button(
                    onPressed: () async {
                      if (await confirm(
                        '停止千变后台？',
                        '机器人连接与插件都会停止。关闭管理窗口不会停止后台。',
                      )) {
                        await action(() async {
                          await widget.api.request('/shutdown', method: 'POST');
                          await message('后台正在停止');
                        });
                      }
                    },
                    style: ButtonStyle(
                      foregroundColor: WidgetStatePropertyAll(
                        colors.dark
                            ? const Color(0xffffa7af)
                            : const Color(0xffb64c60),
                      ),
                    ),
                    child: Row(
                      mainAxisSize: MainAxisSize.min,
                      children: [
                        Icon(glyph('power_button'), size: 16),
                        const SizedBox(width: 8),
                        const Text('停止后台'),
                      ],
                    ),
                  ),
                ],
              ),
            ]),
          ],
        ),
      ),
    );
  }
}

class ChatEntry {
  ChatEntry(this.outgoing, this.text, this.sender) : time = DateTime.now();
  final bool outgoing;
  final String text, sender;
  final DateTime time;
}

class ChatPage extends StatefulWidget {
  const ChatPage({super.key, required this.api});
  final Api api;
  @override
  State<ChatPage> createState() => _ChatPageState();
}

class _ChatPageState extends State<ChatPage> {
  final input = TextEditingController(),
      user = TextEditingController(text: '调查员'),
      group = TextEditingController(text: '试验团');
  final List<ChatEntry> messages = [];
  final scroll = ScrollController();
  bool busy = false;
  @override
  void dispose() {
    scroll.dispose();
    input.dispose();
    user.dispose();
    group.dispose();
    super.dispose();
  }

  Future<void> send() async {
    final text = input.text.trim();
    if (text.isEmpty || busy) return;
    input.clear();
    setState(() {
      busy = true;
      messages.add(ChatEntry(true, text, user.text));
    });
    try {
      final result = await widget.api.request(
        '/simulate',
        method: 'POST',
        body: {
          'context': {
            'platform': 'simulation',
            'account': 'sandbox',
            'user': user.text,
            'group': group.text,
            'name': user.text,
          },
          'text': text,
        },
      );
      if (mounted)
        setState(
          () => messages.add(
            ChatEntry(
              false,
              '${result['public']}${result['private'] == null ? '' : '\n【仅自己可见】${result['private']}'}',
              '千变',
            ),
          ),
        );
    } catch (e) {
      if (mounted) setState(() => messages.add(ChatEntry(false, '$e', '千变')));
    } finally {
      if (mounted)
        setState(() {
          busy = false;
          WidgetsBinding.instance.addPostFrameCallback((_) {
            if (scroll.hasClients)
              scroll.jumpTo(scroll.position.maxScrollExtent);
          });
          if (messages.length > 200)
            messages.removeRange(0, messages.length - 200);
        });
    }
  }

  Future<void> editIdentity() async {
    final player = TextEditingController(text: user.text);
    final room = TextEditingController(text: group.text);
    final save = await showDialog<bool>(
      context: context,
      builder: (ctx) => ContentDialog(
        title: const Text('练习身份'),
        content: Column(
          mainAxisSize: MainAxisSize.min,
          children: [
            field('玩家', TextBox(controller: player)),
            field('练习群', TextBox(controller: room)),
          ],
        ),
        actions: [
          Button(
            onPressed: () => Navigator.pop(ctx, false),
            child: const Text('取消'),
          ),
          FilledButton(
            onPressed: () => Navigator.pop(ctx, true),
            child: const Text('保存'),
          ),
        ],
      ),
    );
    if (save == true && mounted)
      setState(() {
        user.text = player.text;
        group.text = room.text;
      });
    player.dispose();
    room.dispose();
  }

  Widget messageTile(ChatEntry item) {
    final colors = WorkspacePalette.of(context);
    final avatar = item.outgoing
        ? Container(
            width: 30,
            height: 30,
            decoration: BoxDecoration(
              color: colors.selected,
              shape: BoxShape.circle,
            ),
            child: Icon(glyph('contact'), size: 14, color: colors.accent),
          )
        : const DiceMark(size: 30);
    final stamp =
        '${item.time.hour.toString().padLeft(2, '0')}:${item.time.minute.toString().padLeft(2, '0')}';
    final bubble = Flexible(
      child: Column(
        crossAxisAlignment: item.outgoing
            ? CrossAxisAlignment.end
            : CrossAxisAlignment.start,
        children: [
          Text(
            '${item.sender}  ·  $stamp',
            style: TextStyle(fontSize: 12, color: colors.muted),
          ),
          const SizedBox(height: 8),
          Container(
            padding: const EdgeInsets.symmetric(horizontal: 16, vertical: 13),
            constraints: const BoxConstraints(maxWidth: 540),
            decoration: BoxDecoration(
              color: item.outgoing ? colors.selected : colors.section,
              borderRadius: BorderRadius.only(
                topLeft: const Radius.circular(16),
                topRight: const Radius.circular(16),
                bottomLeft: Radius.circular(item.outgoing ? 16 : 5),
                bottomRight: Radius.circular(item.outgoing ? 5 : 16),
              ),
            ),
            child: SelectableText(
              item.text,
              style: TextStyle(fontSize: 14, height: 1.65, color: colors.text),
            ),
          ),
        ],
      ),
    );
    return Padding(
      padding: const EdgeInsets.only(bottom: 18),
      child: Row(
        mainAxisAlignment: item.outgoing
            ? MainAxisAlignment.end
            : MainAxisAlignment.start,
        crossAxisAlignment: CrossAxisAlignment.start,
        children: item.outgoing
            ? [bubble, const SizedBox(width: 12), avatar]
            : [avatar, const SizedBox(width: 12), bubble],
      ),
    );
  }

  @override
  Widget build(BuildContext context) {
    final colors = WorkspacePalette.of(context);
    return Padding(
      padding: const EdgeInsets.fromLTRB(24, 0, 24, 16),
      child: Column(
        children: [
          Padding(
            padding: const EdgeInsets.only(bottom: 16),
            child: Row(
              children: [
                Expanded(
                  child: Button(
                    onPressed: busy ? null : editIdentity,
                    style: ButtonStyle(
                      backgroundColor: const WidgetStatePropertyAll(
                        Colors.transparent,
                      ),
                      padding: const WidgetStatePropertyAll(
                        EdgeInsets.symmetric(horizontal: 4, vertical: 8),
                      ),
                    ),
                    child: Row(
                      children: [
                        Icon(glyph('contact'), size: 15, color: colors.accent),
                        const SizedBox(width: 8),
                        Flexible(
                          child: Text(
                            '${user.text} · ${group.text}',
                            overflow: TextOverflow.ellipsis,
                            style: TextStyle(fontSize: 13, color: colors.muted),
                          ),
                        ),
                        const SizedBox(width: 8),
                        Icon(glyph('edit'), size: 12, color: colors.muted),
                      ],
                    ),
                  ),
                ),
                const SizedBox(width: 12),
                Text(
                  '仅练习 · 不发送到 QQ',
                  style: TextStyle(fontSize: 12, color: colors.muted),
                ),
              ],
            ),
          ),
          Expanded(
            child: Container(
              decoration: BoxDecoration(
                color: colors.section.withValues(
                  alpha: colors.dark ? .45 : .55,
                ),
                borderRadius: BorderRadius.circular(24),
                border: Border.all(color: colors.edge),
              ),
              child: messages.isEmpty
                  ? Center(
                      child: Padding(
                        padding: const EdgeInsets.all(24),
                        child: Column(
                          mainAxisSize: MainAxisSize.min,
                          children: [
                            const DiceMark(size: 64),
                            const SizedBox(height: 24),
                            Text(
                              '试一条指令',
                              style: TextStyle(
                                fontSize: 23,
                                fontWeight: FontWeight.w600,
                                color: colors.text,
                              ),
                            ),
                            const SizedBox(height: 24),
                            Wrap(
                              spacing: 10,
                              runSpacing: 10,
                              alignment: WrapAlignment.center,
                              children: [
                                for (final command in [
                                  '.r 1d100',
                                  '.coc',
                                  '.help',
                                ])
                                  commandButton(
                                    command,
                                    () => input.text = command,
                                  ),
                              ],
                            ),
                          ],
                        ),
                      ),
                    )
                  : Align(
                      alignment: Alignment.topCenter,
                      child: ConstrainedBox(
                        constraints: const BoxConstraints(maxWidth: 720),
                        child: ListView.builder(
                          controller: scroll,
                          padding: const EdgeInsets.fromLTRB(20, 24, 20, 8),
                          itemCount: messages.length,
                          itemBuilder: (context, index) =>
                              messageTile(messages[index]),
                        ),
                      ),
                    ),
            ),
          ),
          const SizedBox(height: 16),
          Surface(
            padding: const EdgeInsets.all(8),
            child: Row(
              children: [
                const SizedBox(width: 8),
                Expanded(
                  child: TextBox(
                    controller: input,
                    onSubmitted: (_) => send(),
                    placeholder: '输入指令，如 .r 1d100',
                    padding: const EdgeInsets.symmetric(
                      horizontal: 12,
                      vertical: 14,
                    ),
                    decoration: const WidgetStatePropertyAll(
                      BoxDecoration(
                        color: Colors.transparent,
                        border: Border.fromBorderSide(BorderSide.none),
                      ),
                    ),
                    foregroundDecoration: const WidgetStatePropertyAll(
                      BoxDecoration(
                        border: Border.fromBorderSide(BorderSide.none),
                      ),
                    ),
                  ),
                ),
                const SizedBox(width: 8),
                FilledButton(
                  onPressed: busy ? null : send,
                  child: busy
                      ? const SizedBox.square(
                          dimension: 16,
                          child: ProgressRing(strokeWidth: 2),
                        )
                      : const Text('发送'),
                ),
              ],
            ),
          ),
          const SizedBox(height: 8),
          Align(
            alignment: Alignment.centerRight,
            child: Text(
              'Enter 发送',
              style: TextStyle(fontSize: 11, color: colors.muted),
            ),
          ),
        ],
      ),
    );
  }
}

typedef JsonEditor =
    Future<void> Function(String, dynamic, Future<void> Function(dynamic));
typedef Prompt =
    Future<String?> Function(String, {String initial, String? hint});

class ContentPage extends StatefulWidget {
  const ContentPage({
    super.key,
    required this.api,
    required this.kind,
    required this.edit,
    required this.prompt,
  });
  final Api api;
  final String kind;
  final JsonEditor edit;
  final Prompt prompt;
  @override
  State<ContentPage> createState() => _ContentPageState();
}

class _ContentPageState extends State<ContentPage> {
  List<dynamic> names = [];
  String? error;
  @override
  void initState() {
    super.initState();
    load();
  }

  Future<void> load() async {
    try {
      final value = await widget.api.request('/content/${widget.kind}');
      if (mounted)
        setState(() {
          names = value;
          error = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  Future<void> edit(String name, {bool create = false}) async {
    try {
      final value = create
          ? (widget.kind == 'decks'
                ? {
                    'without_replacement': false,
                    'entries': [
                      {'text': '新的故事开始了。', 'weight': 1},
                    ],
                  }
                : {
                    'id': name.replaceAll('.json', ''),
                    'label': '自定义规则',
                    'faces': 100,
                    'comparison': 'lte',
                    'critical': 1,
                    'fumble': 100,
                  })
          : await widget.api.request(
              '/content/${widget.kind}/${Uri.encodeComponent(name)}',
            );
      await widget.edit(name, value, (updated) async {
        await widget.api.request(
          '/content/${widget.kind}/${Uri.encodeComponent(name)}',
          method: 'PUT',
          body: updated,
        );
        await load();
      });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    children: [
      Row(
        children: [
          commandButton(
            '新建',
            () async {
              final name = await widget.prompt(
                '新建${widget.kind == 'decks' ? '牌堆' : '规则'}',
                hint: '名称.json',
              );
              if (name == null || name.isEmpty) return;
              await edit(
                name.endsWith('.json') ? name : '$name.json',
                create: true,
              );
            },
            icon: 'add',
            primary: true,
          ),
          const Spacer(),
          Tooltip(
            message: '刷新',
            child: IconButton(
              onPressed: load,
              icon: Icon(glyph('refresh'), size: 16),
            ),
          ),
        ],
      ),
      const SizedBox(height: 20),
      if (error != null)
        InfoBar(
          title: const Text('操作未完成'),
          content: Text(error!),
          severity: InfoBarSeverity.error,
        ),
      for (final name in names)
        Padding(
          padding: const EdgeInsets.only(bottom: 10),
          child: Surface(
            child: ListTile(
              leading: Icon(
                glyph(widget.kind == 'decks' ? 'album' : 'library'),
                size: 20,
              ),
              title: Text('$name'),
              trailing: Icon(glyph('chevron_right'), size: 12),
              onPressed: () => edit('$name'),
            ),
          ),
        ),
      if (names.isEmpty)
        const Padding(
          padding: EdgeInsets.all(40),
          child: Center(child: Text('暂无自定义内容')),
        ),
    ],
  );
}

class LogsPage extends StatefulWidget {
  const LogsPage({super.key, required this.api, required this.worlds});
  final Api api;
  final List<dynamic> worlds;
  @override
  State<LogsPage> createState() => _LogsPageState();
}

class _LogsPageState extends State<LogsPage> {
  final scope = TextEditingController(), session = TextEditingController();
  List<dynamic> rows = [], sessions = [];
  int? selected;
  String? error, notice;
  @override
  void initState() {
    super.initState();
    loadSessions();
  }

  Future<void> loadSessions() async {
    try {
      final value = await widget.api.request('/sessions');
      if (mounted)
        setState(() {
          sessions = value;
          if (selected != null && selected! >= sessions.length) selected = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  void dispose() {
    scope.dispose();
    session.dispose();
    super.dispose();
  }

  String query([String format = 'txt']) => Uri(
    queryParameters: {
      'scope': scope.text,
      'session': session.text,
      'format': format,
    },
  ).query;
  Future<void> load({bool more = false}) async {
    try {
      final result = await widget.api.request(
        '/logs?${query()}&after=${more && rows.isNotEmpty ? rows.last['id'] : 0}',
      );
      if (mounted)
        setState(() {
          rows = more
              ? [
                  ...rows.skip(rows.length > 800 ? rows.length - 800 : 0),
                  ...result,
                ]
              : result;
          error = null;
        });
    } catch (e) {
      if (mounted) setState(() => error = '$e');
    }
  }

  @override
  Widget build(BuildContext context) => ListView(
    padding: const EdgeInsets.fromLTRB(24, 8, 24, 24),
    children: [
      Row(
        children: [
          Expanded(
            child: field(
              '历史团录',
              ComboBox<int>(
                isExpanded: true,
                value: selected,
                placeholder: const Text('选择团录'),
                items: [
                  for (var index = 0; index < sessions.length; index++)
                    ComboBoxItem(
                      value: index,
                      child: Text(
                        '${sessions[index]['session']} · ${sessions[index]['count']} 条',
                      ),
                    ),
                ],
                onChanged: (index) {
                  if (index == null) return;
                  setState(() => selected = index);
                  scope.text = '${sessions[index]['scope']}';
                  session.text = '${sessions[index]['session']}';
                  load();
                },
              ),
            ),
          ),
          const SizedBox(width: 12),
          Tooltip(
            message: '刷新',
            child: IconButton(
              onPressed: loadSessions,
              icon: Icon(glyph('refresh'), size: 16),
            ),
          ),
        ],
      ),
      field(
        '账号数据',
        ComboBox<String>(
          isExpanded: true,
          value: widget.worlds.any((world) => '${world['key']}' == scope.text)
              ? scope.text
              : null,
          items: widget.worlds
              .map<ComboBoxItem<String>>(
                (world) => ComboBoxItem(
                  value: '${world['key']}',
                  child: Text('${world['key']}'),
                ),
              )
              .toList(),
          onChanged: (value) => setState(() => scope.text = value ?? ''),
        ),
      ),
      field('日志名称', TextBox(controller: session, placeholder: '.log on 使用的名称')),
      Wrap(
        spacing: 12,
        runSpacing: 12,
        children: [
          commandButton('查看记录', load, primary: true),
          for (final format in ['txt', 'html', 'json'])
            commandButton('导出 ${format.toUpperCase()}', () async {
              try {
                final bytes = await widget.api.download(
                  '/export?${query(format)}',
                );
                final saved = await platform.save(
                  bytes,
                  'qianbian-log.$format',
                  format == 'html'
                      ? 'text/html'
                      : format == 'json'
                      ? 'application/json'
                      : 'text/plain',
                );
                if (mounted && saved) setState(() => notice = '团录已导出');
              } catch (e) {
                if (mounted) setState(() => error = '$e');
              }
            }, icon: 'download'),
        ],
      ),
      const SizedBox(height: 20),
      if (error != null)
        InfoBar(
          title: const Text('操作未完成'),
          content: Text(error!),
          severity: InfoBarSeverity.error,
          onClose: () => setState(() => error = null),
        ),
      if (notice != null)
        InfoBar(
          title: Text(notice!),
          severity: InfoBarSeverity.success,
          onClose: () => setState(() => notice = null),
        ),
      for (final row in rows)
        ListTile(
          title: SelectableText('${row['text']}'),
          subtitle: Text('${row['actor']} · ${readableTime(row['time'])}'),
        ),
      if (rows.isNotEmpty)
        Button(onPressed: () => load(more: true), child: const Text('加载更多')),
    ],
  );
}
