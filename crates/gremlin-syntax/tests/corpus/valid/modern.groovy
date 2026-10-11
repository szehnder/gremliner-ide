// TinkerPop reference-style traversals over the "modern" toy graph.

g.V()

g.E()

g.V(1)

g.V(1).values('name')

g.V(1).outE('knows')

g.V(1).outE('knows').inV().values('name')

g.V(1).out('knows').values('name')

g.V(1).out('knows').has('age', gt(30)).values('name')

g.V().hasLabel('person')

g.V().has('person', 'name', 'marko')

g.V().has('name', 'marko').out('created').values('name')

g.V().has('name', 'marko').out('created').in('created').values('name')

g.V().has('name', 'marko').out('created').in('created').where(neq('marko')).values('name')

g.V().as('a').out('created').in('created').where(neq('a')).values('name')

g.V().has('name', within('vadas', 'marko')).values('age')

g.V().has('name', within('vadas', 'marko')).values('age').mean()

g.V().hasLabel('person').values('age').sum()

g.V().hasLabel('person').values('age').max()

g.V().hasLabel('person').values('age').min()

g.V().hasLabel('person').count()

g.V().label().groupCount()

g.V().group().by(label)

g.V().group().by(label).by('name')

g.V().group().by(label).by(count())

g.V().groupCount().by(label)

g.V().hasLabel('software').values('name', 'lang')

g.V().valueMap()

g.V().valueMap('name', 'age')

g.V().valueMap().with(WithOptions.tokens)

g.V().elementMap()

g.V().elementMap('name')

g.E().elementMap()

g.V().properties('name')

g.V().properties().key().dedup()

g.V().propertyMap()

g.V().id()

g.V().label()

g.V().out().out().values('name')

g.V().repeat(out()).times(2).values('name')

g.V(1).repeat(out()).until(hasLabel('software')).path().by('name')

g.V(1).repeat(out()).emit().path().by('name')

g.V(1).emit().repeat(out()).times(2).path().by('name')

g.V().repeat(both()).times(3).dedup().count()

g.V(1).repeat(both().simplePath()).until(hasId(6)).path().limit(1)

g.V().both().both().cyclicPath().path()

g.V().out().simplePath().path()

g.V().outE().inV().path()

g.V().outE().inV().path().by('name').by(label)

g.V().out('knows').path().by(valueMap('name'))

g.V(1).out().out().path().by('name')

g.V().where(out('created')).values('name')

g.V().where(out('created').count().is(gt(1))).values('name')

g.V().not(out('created')).values('name')

g.V().and(outE('knows'), values('age').is(lt(30))).values('name')

g.V().or(has('age', gt(30)), has('lang', 'java')).values('name')

g.V().hasLabel('person').order().by('age', desc).values('name')

g.V().hasLabel('person').order().by('age', asc).limit(2).values('name')

g.V().order().by('name').values('name')

g.V().order().by(shuffle).limit(1)

g.V().hasLabel('person').order().by(outE().count(), desc)

g.V().values('name').order().by(desc)

g.V().range(0, 2)

g.V().skip(2).limit(2)

g.V().tail(2)

g.V().values('age').fold()

g.V().values('age').fold().unfold()

g.V().values('age').fold().count(local)

g.V().values('name').fold().order(local).by(desc)

g.V().hasLabel('person').project('name', 'age').by('name').by('age')

g.V().hasLabel('person').project('name', 'knows').by('name').by(out('knows').count())

g.V().project('v', 'degree').by().by(bothE().count())

g.V().as('a').out().as('b').select('a', 'b')

g.V().as('a').out().as('b').select('a', 'b').by('name')

g.V().as('a').out('created').as('b').select('a', 'b').by('name').by('lang')

g.V().choose(hasLabel('person'), values('age'), values('lang'))

g.V().choose(label()).option('person', values('name')).option('software', values('lang'))

g.V().choose(values('age').is(lte(30)), __.in(), __.out()).values('name')

g.V().coalesce(outE('knows'), inE('created')).count()

g.V().optional(out('knows')).values('name')

g.V().union(out('created'), out('knows')).values('name')

g.V(4).union(__.in().values('age'), out().values('lang'))

g.V().local(outE().limit(1)).inV().values('name')

g.V().local(properties('name').count())

g.V().map(values('name'))

g.V().flatMap(out('knows'))

g.V().filter(has('age', gt(30))).values('name')

g.V().sideEffect(out('created').aggregate('x')).cap('x')

g.V(1).out('created').aggregate('x').in('created').out('created').where(without('x')).values('name')

g.V().out('knows').aggregate('friends').cap('friends')

g.V().aggregate(local, 'x').cap('x')

g.V().values('name').is(TextP.startingWith('m'))

g.V().has('name', TextP.containing('a'))

g.V().has('name', TextP.regex('^[mj].*'))

g.V().has('age', between(20, 35))

g.V().has('age', inside(20, 35)).values('name')

g.V().has('age', outside(20, 35)).values('name')

g.V().has('age', P.gte(29).and(P.lt(35)))

g.V().hasNot('age').values('name')

g.V().has('age').values('name')

g.V().hasId(1, 2, 3)

g.V().hasLabel('person', 'software').count()

g.E().hasLabel('knows').has('weight', gt(0.5))

g.E().has('weight', 1.0d).outV().values('name')

g.V().bothE().otherV().dedup().values('name')

g.V(1).bothE().where(otherV().hasId(2))

g.V().inE('created').outV().dedup().values('name')

g.V().coin(0.5)

g.V().sample(2)

g.V().constant('x')

g.V().identity()

g.V().math('_ + 1').by('age')

g.V().as('a').out('knows').as('b').math('a - b').by('age')

g.V().hasLabel('person').values('age').math('_ * 2')

g.V().barrier().count()

g.V().dedup().by(label).values('name')

g.V().out().dedup().values('name')

g.V().both().groupCount().order(local).by(values, desc).limit(local, 1)

g.V().groupCount().by(both().count()).unfold()

g.V().hasLabel('person').group().by(values('age').is(gt(30))).by('name')

g.V().match(__.as('a').out('created').as('b'), __.as('b').has('name', 'lop'), __.as('b').in('created').as('c')).select('a', 'c').by('name')

g.V().match(__.as('a').out('knows').as('b')).select('a', 'b').by('name')

g.V().branch(values('name')).option('marko', values('age')).option(none, values('name'))

g.V().where(__.in('created').count().is(1)).values('name')

g.V().where(outE().count().is(eq(0))).values('name')

g.V(1).as('a').out('created').in('created').where(neq('a')).dedup().values('name')

g.V().hasLabel('person').where(values('age').is(gt(30))).count()

g.V().tree()

g.V().out().out().tree().by('name')

g.V().outE('knows').subgraph('sg').cap('sg')

g.V().inject(1, 2, 3)

g.inject(1, 2, 3).sum()

g.inject('a', 'b').fold()

g.inject([1, 2, 3]).unfold().order().by(desc)

g.V().count().is(6)

g.V().hasLabel('person').values('name').toList()

g.V().hasLabel('person').next()

g.V().hasLabel('person').next(2)

g.V(1).hasNext()

g.V(99).tryNext()

g.V().toSet()

g.V().values('name').toBulkSet()

g.V().out().explain()

g.V().out().profile()

g.V().out().out().count().profile()

g.V().timeLimit(100).count()

g.V().loops()

g.V().repeat(out()).until(loops().is(2)).values('name')

g.V().repeat('a', out()).until(loops('a').is(2)).values('name')

g.withSack(1.0f).V().repeat(outE().sack(mult).by('weight').inV()).times(2).sack()

g.withSideEffect('x', []).V().out().aggregate('x').cap('x')

g.withComputer().V().pageRank().with(PageRank.propertyName, 'rank').values('rank')

g.withComputer().V().peerPressure().with(PeerPressure.propertyName, 'cluster').values('cluster')

g.withComputer().V().connectedComponent().with(ConnectedComponent.propertyName, 'component').project('name', 'component').by('name').by('component')

g.withComputer().V().shortestPath().with(ShortestPath.target, __.has('name', 'peter'))

g.V().hasLabel('person').values('name').concat(' is a person')

g.V().values('name').toUpper()

g.V().values('name').toLower()

g.V().values('name').length()

g.V().values('name').substring(1, 3)

g.V().values('name').replace('a', 'A')

g.V().values('name').split('a')

g.V().values('name').reverse()

g.V().values('name').trim()

g.V().values('name').lTrim()

g.V().values('name').rTrim()

g.V().values('name').fold().conjoin(', ')

g.inject('  hello  ').trim().toUpper()

g.V().hasLabel('person').format('%{name} is %{age} years old')

g.inject(datetime('2023-08-02T00:00:00Z')).dateAdd(DT.day, 7)

g.inject(datetime('2023-08-02T00:00:00Z')).dateDiff(datetime('2023-08-09T00:00:00Z'))

g.inject('2023-08-02').asDate()

g.inject(1).asString()

g.inject([1, 2, 3]).combine([4, 5])

g.inject([1, 2, 3]).merge([3, 4])

g.inject([1, 2, 3]).intersect([2, 3, 4])

g.inject([1, 2, 3]).difference([2])

g.inject([1, 2, 3]).disjunct([3, 4])

g.inject([1, 2]).product(['a', 'b'])

g.inject([1, 2, 3]).all(gt(0))

g.inject([1, 2, 3]).any(gt(2))

g.V().none()

g.V().values('name').fold().index()

g.V().hasLabel('person').values('name').none()

g.V().element()

g.V().properties('name').element().label()

g.V().fail('should not get here').count()

g.call()

g.call('tinker.search', [search: 'mark'])

g.V().has('name', 'marko').sideEffect(__.out().count().aggregate(local, 'c')).values('name')

g.V().hasLabel('person').as('p').out('knows').as('f').select('p', 'f').by(valueMap('name'))

g.V().has('person', 'name', 'marko').outE('knows').order().by('weight', desc).inV().values('name')

g.V().both().has('age').values('age').order().by(desc).limit(3)

g.V().hasLabel('person').values('age').order().fold().unfold().limit(1)

g.V().hasLabel('software').in('created').values('name').dedup().fold()

g.V().has('name', 'josh').out('created').values('name').order()

g.V().id().fold().count(local)

g.V().hasLabel('person').as('p').map(out('created').values('name').fold()).as('s').select('p', 's').by('name').by()

g.V().where(both().count().is(gte(3))).values('name')

g.V().hasLabel('person').has('age', P.not(within(27, 29))).values('name')

g.V().has('name', P.without('marko', 'josh')).values('name')
