from qianbian import serve


def command(request, host):
    user = request['context']['user']
    state = host.call('storage.get', key='greetings')
    count = (state['value'] or 0) + 1
    host.call('storage.put', key='greetings', value=count, revision=state['revision'])
    roll = host.call('dice.roll', expression='1d6')
    return {'public': f'你好，{user}！这是第 {count} 次问好，骰点 {roll["total"]}。'}


serve(command)
