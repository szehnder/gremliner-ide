/* Syntax edge cases: comments, numbers, collections, closures, statements. */

g.V().limit(10L)

g.V().has('weight', 0.4f)

g.V().has('price', 9.99g)

g.V().has('big', 12345678901234567890g)

g.V().has('x', 1e10)

g.V().has('x', -1.5e-3d)

g.V().has('x', 0xFF)

g.V().limit(-1)

g.inject(true, false, null)

g.inject([:])

g.inject([])

g.inject([1, [2, [3]]])

g.inject([a: [b: [c: 1]]])

g.inject(['key': 'value', 1: 'one'])

g.V().has("name", "marko")

g.V().has('name', 'it\'s')

g.V().has("name", "say \"hi\"")

g.V()   .   out( )   .   values( 'name' )

g.V().
  out('knows').
  values('name')

g.V()
  .out('knows')
  .values('name')

g.V().out('knows') // trailing comment

g.V() /* inline */ .count()

g.V().count(); g.E().count()

g.V().filter{ it.get().value('name').startsWith('m') }

g.V().map{ it.get().label() }

g.V().sideEffect{ println it }

g.V().filter{ x -> x.get().property('age').isPresent() && x.get().value('age') > 30 }

g.V().order().by{ a, b -> a.value('name') <=> b.value('name') }

g.V().values('name').map{ "${it.get()}!" }

g.V().hasLabel('person').
  project('name', 'age').
    by('name').
    by('age')

g.V().union(
    __.out('knows'),
    __.out('created')
  ).values('name')

g.V().has('name', 'marko').
  repeat(out().simplePath()).
    times(3).
  path().
    by('name')

g.withStrategies(ReadOnlyStrategy.instance()).V().count()

g.withStrategies(SubgraphStrategy.build().vertices(has('age', gt(30))).create()).V().count()

g.withoutStrategies(org.apache.tinkerpop.gremlin.process.traversal.strategy.optimization.LazyBarrierStrategy).V().count()

g.withBulk(false).V().out().count()

g.withPath().V().out().path()

g.V().order().by(values('name'), Order.desc)

g.V().order().by(Column.keys)

g.V().group().by(T.label).by(T.id)

g.V().values('age').is(P.within([29, 32]))

g.V().select(Pop.last, 'a')

g.V().out().as('x').select(Pop.all, 'x')

g.V().order(Scope.local)

g.V().valueMap().with(WithOptions.tokens, WithOptions.labels)

g.V().valueMap().by(__.unfold())

g.V().properties().hasKey('name').hasValue('marko')

g.V().hasLabel(within('person', 'software'))

g.V().has(T.label, 'person')

g.V().has(T.id, 1)

g.V().where(__.not(__.out()))

g.V().local(__.both().limit(3).fold())

g.V().as('a').out().where(__.in().as('a'))

g.V().choose(__.has('age')).option(true, __.values('age')).option(false, __.constant(0))

g.V().barrier(100).count()

g.V().aggregate(Scope.local, 'x').cap('x')

g.V().sack(Operator.sum).by('age').sack()

g.withSack(0).V().sack(sum).by('age').sack()

g.withSack([]).V().sack(addAll).by(values('name').fold()).sack()

g.V().out().out().path().from('a').to('b')

g.V().group('m').by(label).cap('m')

g.V().groupCount('m').by(label).cap('m')

g.V().tree('t').cap('t')

g.V().count(Scope.global)

g.V().fold().count(Scope.local)

g.V().fold().limit(local, 2)

g.V().fold().range(local, 1, 3)

g.V().fold().tail(local, 1)

g.V().fold().sample(local, 2)

g.V().fold().dedup(local)

g.V().fold().unfold().order().by(Order.shuffle)

g.V().both().both().dedup().by(T.id).count()

g.V().hasLabel('person').as('p').
  out('created').as('s').
  select('p', 's').
    by('name').
    by('lang')

g.V().project('a', 'b', 'c').by(constant(1)).by(constant(2L)).by(constant(3.0d))

g.inject(1).math('sin(_) + cos(_)')

g.inject(10).math('abs(_ - 20) / 2')

g.V().hasLabel('person').values('age').fold(0, sum)

g.V().where(outE().has('weight', gt(0.5d)))

g.V().has('code', 'AUS').outE().where(inV().has('code', 'DFW'))

g.V().hasLabel('person').has('name', neq('marko')).count()

graph.features()

results = g.V().limit(5).toList()
