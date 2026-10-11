// Writes, upserts and schema-ish operations.

g.addV('person').property('name', 'stephen')

g.addV('person').property('name', 'stephen').property('age', 50).next()

g.addV('person').property(T.id, 100).property('name', 'kelvin')

g.addV('airport').property('code', 'XYZ').property('runways', 2).iterate()

g.V().has('name', 'marko').property('age', 30)

g.V(1).property(single, 'age', 31)

g.V(1).property(list, 'nickname', 'mark')

g.V(1).property(set, 'tag', 'founder')

g.V(1).properties('nickname').drop()

g.V(1).properties('name').property('since', 2010)

g.V().has('name', 'stephen').drop()

g.V().hasLabel('temp').drop().iterate()

g.E().hasLabel('knows').has('weight', lt(0.5)).drop()

g.V(1).addE('knows').to(V(2)).property('weight', 0.5d)

g.V(1).as('a').V(2).addE('knows').from('a')

g.addE('route').from(__.V().has('code', 'AUS')).to(__.V().has('code', 'XYZ')).property('dist', 1234)

g.V().has('code', 'AUS').as('a').V().has('code', 'XYZ').addE('route').from('a').property('dist', 1234)

g.mergeV([(T.label): 'person', name: 'marko'])

g.mergeV([(T.label): 'person', name: 'vadas']).option(Merge.onCreate, [age: 27]).option(Merge.onMatch, [updated: true])

g.mergeE([(T.label): 'knows', (Direction.OUT): 1, (Direction.IN): 2])

g.mergeE([(T.label): 'knows', (Direction.OUT): 1, (Direction.IN): 2]).option(Merge.onCreate, [weight: 0.5])

g.V().has('person', 'name', 'josh').fold().coalesce(unfold(), addV('person').property('name', 'josh'))

g.V().has('code', 'XYZ').fold().coalesce(unfold(), addV('airport').property('code', 'XYZ')).property('city', 'Nowhere')

g.inject([[name: 'a'], [name: 'b']]).unfold().as('m').addV('person').property('name', select('m').select('name'))

g.V(1).sideEffect(properties('temp').drop()).values('name')

g.io('data/air-routes.graphml').read().iterate()

g.io('/tmp/out.json').write().iterate()

g.tx().commit()

g.tx().rollback()

def v = g.addV('person').property('name', 'temp').next()

x = g.V(1).next()

g.V(x).property('visited', true)

g.V().has('name', 'marko').property('updated', System.currentTimeMillis())
