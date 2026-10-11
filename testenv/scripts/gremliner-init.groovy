// Gremliner testenv init: load the bundled TinkerPop sample graphs and bind traversal sources.
// air-routes (`graph`) and `synth` are loaded on demand by `cargo xtask testenv seed`.
modern.io(IoCore.gryo()).readGraph('data/tinkerpop-modern.kryo')
crew.io(IoCore.gryo()).readGraph('data/tinkerpop-crew.kryo')

def globals = [:]
globals << [g : traversal().withEmbedded(graph)]
globals << [gModern : traversal().withEmbedded(modern)]
globals << [gCrew : traversal().withEmbedded(crew)]
globals << [gSynth : traversal().withEmbedded(synth)]
