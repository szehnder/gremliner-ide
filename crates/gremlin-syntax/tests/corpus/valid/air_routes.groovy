// Practical Gremlin-style queries over the air-routes graph.

g.V().has('airport', 'code', 'AUS').values('city')

g.V().has('code', 'DFW').out('route').values('code')

g.V().has('code', 'AUS').out('route').count()

g.V().has('code', 'AUS').out().has('country', 'US').values('code').fold()

g.V().hasLabel('airport').has('country', 'US').count()

g.V().hasLabel('airport').groupCount().by('country').order(local).by(values, desc).limit(local, 10)

g.V().hasLabel('airport').has('region', 'US-TX').values('code').order()

g.V().has('code', 'LHR').outE('route').has('dist', gt(5000)).inV().values('code')

g.V().has('code', 'SAF').out().path().by('code')

g.V().has('code', 'AUS').out().out().has('code', 'LHR').path().by('code').limit(5)

g.V().has('code', 'AUS').repeat(out().simplePath()).until(has('code', 'AGR')).limit(3).path().by('code')

g.V().has('code', 'AUS').repeat(out().simplePath()).times(2).has('code', 'SYD').path().by('code').limit(10)

g.V().has('code', 'AUS').outE().order().by('dist', desc).limit(1).inV().values('code')

g.V().has('code', 'AUS').outE('route').values('dist').max()

g.V().has('code', 'AUS').outE('route').values('dist').mean()

g.V().hasLabel('airport').order().by('runways', desc).limit(5).valueMap('code', 'runways')

g.V().hasLabel('airport').has('runways', gte(5)).values('code', 'runways').fold()

g.V().has('airport', 'longest', gt(15000)).values('code')

g.V().hasLabel('airport').where(out().count().is(gt(200))).values('code')

g.V().hasLabel('airport').order().by(out().count(), desc).limit(10).project('code', 'routes').by('code').by(out().count())

g.V().hasLabel('country').has('code', 'FR').out('contains').count()

g.V().hasLabel('continent').project('continent', 'airports').by('desc').by(out('contains').count())

g.V().has('code', 'AUS').in('contains').values('desc')

g.V().has('code', 'AUS').in('contains').hasLabel('country').values('desc')

g.V().has('airport', 'city', 'London').values('code')

g.V().has('airport', 'city', TextP.startingWith('San')).values('city').dedup()

g.V().has('airport', 'desc', TextP.containing('International')).count()

g.V().hasLabel('airport').values('elev').max()

g.V().hasLabel('airport').order().by('elev', desc).limit(1).values('code', 'elev')

g.V().has('code', 'LAX').outE().has('dist', lt(100)).inV().values('code')

g.V().has('code', 'JFK').out().where(out().has('code', 'LHR')).values('code')

g.V().has('code', 'SFO').as('a').out().out().where(eq('a')).count()

g.V().has('code', 'AUS').both().dedup().count()

g.V().has('code', 'AUS').bothE().otherV().dedup().count()

g.V().has('code', 'AUS').as('a').out().as('b').out().as('c').select('a', 'b', 'c').by('code').limit(5)

g.V().has('code', 'AUS').out().in().where(neq('AUS')).dedup().count()

g.V().hasLabel('airport').sample(5).values('code')

g.V().hasLabel('airport').coin(0.01).values('code')

g.V().has('code', 'AUS').outE().inV().path().by('code').by('dist').limit(3)

g.V().has('code', 'AUS').outE().as('e').inV().as('v').select('e', 'v').by('dist').by('code').limit(5)

g.V().has('code', 'ATL').out().has('country', within('MX', 'CA')).values('code')

g.V().hasLabel('airport').has('country', without('US', 'CA')).count()

g.V().has('code', 'AUS').choose(values('runways').is(gt(1)), constant('big'), constant('small'))

g.V().has('code', within('AUS', 'DFW', 'IAH')).project('code', 'out').by('code').by(out().count())

g.V().has('code', 'AUS').local(out().order().by('code').limit(3)).values('code')

g.V().hasLabel('airport').limit(10).local(outE().order().by('dist', desc).limit(1)).inV().values('code')

g.V().hasLabel('airport').group().by('country').by(count()).unfold().order().by(values, desc).limit(5)

g.V().hasLabel('airport').values('country').groupCount().order(local).by(values, desc).select(keys).limit(local, 5)

g.V().has('code', 'AUS').out('route').values('code').fold().count(local)

g.V().has('code', 'AUS').out('route').values('code').order().fold().index()

g.V().has('code', 'AUS').values('code', 'city', 'region').fold()

g.V().has('code', 'AUS').valueMap('code', 'city').by(unfold())

g.V().has('code', 'AUS').elementMap('code', 'city')

g.V().hasLabel('airport').has('code', 'AUS').properties().count()

g.V().hasLabel('airport').has('code', 'AUS').properties('city').value()

g.E().hasLabel('route').count()

g.E().hasLabel('route').has('dist', gt(8000)).count()

g.E().hasLabel('route').order().by('dist', desc).limit(1).project('from', 'to', 'dist').by(outV().values('code')).by(inV().values('code')).by('dist')

g.V().has('code', 'AUS').outE('route').inV().has('code', 'LHR').path().by('code').by('dist')

g.V().has('code', 'AUS').repeat(out('route')).emit().times(2).dedup().count()

g.V().has('code', 'AUS').repeat(out('route').simplePath()).until(has('code', 'WLG').or().loops().is(4)).has('code', 'WLG').path().by('code').limit(1)

g.V().has('code', 'AUS').sack(assign).by(constant(0)).repeat(outE().sack(sum).by('dist').inV().simplePath()).times(2).has('code', 'LHR').sack().min()

g.withSack(0).V().has('code', 'AUS').repeat(outE().sack(sum).by('dist').inV().simplePath()).times(2).has('code', 'MEL').order().by(sack()).limit(1).path().by('code').by('dist')

g.V().has('code', 'AUS').out().has('code', P.neq('DFW')).values('code').limit(5)

g.V().hasLabel('airport').where(values('runways').is(gt(3))).where(values('elev').is(gt(5000))).values('code')

g.V().hasLabel('airport').filter(values('city').is(TextP.endingWith('ville'))).values('city')

g.V().has('code', 'AUS').optional(out().has('code', 'XXX')).values('code')

g.V().has('code', 'AUS').coalesce(out().has('code', 'XXX'), constant('none'))

g.V().has('code', 'AUS').union(out().has('country', 'MX').values('code'), out().has('country', 'CA').values('code'))

g.V().hasLabel('airport').has('country', 'US').values('region').dedup().count()

g.V().hasLabel('airport').has('country', 'US').group().by('region').by('code').select('US-TX')

g.V().has('code', 'AUS').out().group().by('country').by(count())

g.V().has('code', 'AUS').out().groupCount().by('country').unfold().order().by(values, desc)

g.V().hasLabel('airport').as('a').out().as('b').where('a', eq('b')).by('country').count()

g.V().has('code', 'AUS').as('start').out().out().where(neq('start')).dedup().count()

g.V().hasLabel('airport').has('code', 'AUS').math('elev / 3.28').by('elev')

g.V().has('code', 'AUS').project('code', 'elevMeters').by('code').by(math('_ / 3.28').by('elev'))

g.V().hasLabel('airport').limit(5).project('code', 'routes', 'runways').by('code').by(outE('route').count()).by('runways')

g.V().has('code', 'AUS').outE('route').group().by(inV().values('country')).by(values('dist').mean())

g.V().has('code', 'AUS').out().values('code').is(within('DFW', 'IAH', 'DAL'))

g.V().has('code', 'AUS').out().has('code', TextP.regex('^D.*')).values('code')

g.V().has('code', 'AUS').out().values('city').dedup().order().limit(10)

g.V().has('code', 'AUS').out().values('city').order().by(desc).limit(1)

g.V().has('code', 'AUS').out().order().by('elev', desc).limit(3).values('code')

g.V().has('code', 'AUS').out().count().math('_ * 2')

g.V().has('code', 'AUS').both('route').where(both('route').has('code', 'LHR')).values('code')

g.V().hasLabel('airport').has('code', 'AUS').emit().repeat(out()).times(1).dedup().count()

g.V().hasLabel('airport').where(outE().count().is(0)).values('code')

g.V().hasLabel('airport').not(__.in('route')).values('code')

g.V().has('code', 'AUS').out().simplePath().path().by('code').limit(3)

g.V().has('code', 'AUS').out().out().cyclicPath().path().by('code').limit(3)

g.V().has('code', 'AUS').out().tail(3).values('code')

g.V().has('code', 'AUS').out().range(5, 10).values('code')

g.V().has('code', 'AUS').out().skip(5).limit(5).values('code')

g.V().has('code', 'AUS').out().dedup().by('country').values('country')

g.V().has('code', 'AUS').out().aggregate('a').out().where(within('a')).dedup().count()

g.V().has('code', 'AUS').out().aggregate(local, 'a').out().where(without('a')).dedup().count()

g.V().has('code', 'AUS').out().values('code').fold().as('codes').select('codes').count(local)

g.V().has('code', 'AUS').project('code', 'neighbours').by('code').by(out().values('code').fold())

g.V().has('code', 'AUS').map(out().count())

g.V().has('code', 'AUS').flatMap(out().limit(3)).values('code')

g.V().has('code', 'AUS').local(out().limit(3).fold())

g.V().has('code', 'AUS').out().choose(values('country')).option('US', constant('domestic')).option(none, constant('international')).groupCount()

g.V().has('code', 'AUS').out().branch(values('country')).option('MX', values('code')).option('CA', values('city'))

g.V().has('code', 'AUS').outE().has('dist', between(100, 200)).inV().values('code')

g.V().has('code', 'AUS').outE().order().by('dist').limit(1).inV().values('code')

g.V().has('code', 'AUS').outE().values('dist').order().limit(1)

g.V().has('code', 'AUS').outE().values('dist').sum()

g.V().has('code', 'AUS').outE().count().is(gt(50))

g.V().has('code', 'AUS').bothE().dedup().count()

g.V().has('code', 'AUS').inE().outV().values('code').fold()

g.V().hasLabel('version').valueMap()

g.V().hasLabel('airport').has('code', 'AUS').id()

g.V().hasLabel('airport').has('code', 'AUS').label()

g.V().has('code', 'AUS').outE().label().dedup()

g.V().has('code', 'AUS').outE().properties().key().dedup()

g.V().has('code', 'AUS').out().has('lat', gt(40.0)).values('code')

g.V().has('code', 'AUS').out().has('lon', lt(-100.0)).count()

g.V().hasLabel('airport').has('lat', between(29.0, 31.0)).has('lon', between(-99.0, -97.0)).values('code')
