const hyperswarm = require('hyperswarm')

const opts = {
  runBootstrap: false
}
main(opts).catch(console.error)

async function main (opts = {}) {
  let bootstrap
  if (opts.runBootstrap) {
    bootstrap = await bootstrapDHT(6060)
  } else {
    bootstrap = 'localhost:6060'
  }
  console.log({ bootstrap })

  const topic = Buffer.alloc(32, 0)

  const swarm1 = runNode(bootstrap, 'node1')
  const swarm2 = runNode(bootstrap, 'node2')

  const config = { announce: true, lookup: true }
  swarm1.join(topic, config)
  swarm2.join(topic, config)
}

function runNode (bootstrap, name) {
  const swarm = hyperswarm({
    announceLocalAddress: true,
    bootstrap: [bootstrap]
  })

  swarm.on('connection', (socket, info) => {
    const peer = info.peer
    let peerAddr = peer ? `${peer.host}:${peer.port}` : 'unknown'
    const label = `[${name} -> ${info.type}://${peerAddr}]`
    console.log(`${label} connect`)
    socket.write(Buffer.from(`hi from ${name}!`))
    socket.on('data', buf => {
      console.log(`${label} read: ${buf.toString()}`)
    })
    socket.on('error', err => {
      console.log(`${label} error: ${err.toString()}`)
    })
    socket.on('close', () => {
      console.log(`${label} close`)
    })
  })

  return swarm
}

async function bootstrapDHT (port) {
  const bootstrapper = require('@hyperswarm/dht')({
    bootstrap: false
  })
  bootstrapper.listen(port)
  await new Promise(resolve => {
    return bootstrapper.once('listening', resolve)
  })
  const bootstrapPort = bootstrapper.address().port
  const bootstrapAddr = `localhost:${bootstrapPort}`
  console.log(`bootstrap node running on ${bootstrapAddr}`)
  return bootstrapAddr
}
