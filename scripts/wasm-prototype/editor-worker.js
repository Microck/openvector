const engine = createTracer();
engine.then(() => postMessage({ready: true})).catch(error => postMessage({fatal: String(error)}));
onmessage = async ({data}) => {
  const {FS, _editor_command} = await engine;
  const {input, ...command} = data;
  try {
    if (input) FS.writeFile('input', new Uint8Array(input));
    FS.writeFile('command.json', JSON.stringify(command));
    const start = performance.now();
    if (_editor_command() < 0) throw new Error(FS.readFile('error.txt', {encoding: 'utf8'}));
    const reply = JSON.parse(FS.readFile('response.json', {encoding: 'utf8'}));
    reply.ms = performance.now() - start;
    const transfers = [];
    for (const [key, file] of [['svg', 'output.svg'], ['pixels', 'pixels']]) {
      if (!reply[key]) continue;
      reply[key] = FS.readFile(file);
      transfers.push(reply[key].buffer);
    }
    postMessage(reply, transfers);
  } catch (error) {
    postMessage({error: String(error)});
  } finally {
    for (const file of ['input', 'command.json', 'response.json', 'error.txt', 'output.svg', 'pixels']) {
      if (FS.analyzePath(file).exists) FS.unlink(file);
    }
  }
};
